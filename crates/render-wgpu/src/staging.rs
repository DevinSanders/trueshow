//! Non-blocking 3-slot asynchronous GPU-to-CPU staging buffer ring for preview & broadcast readback.
//!
//! Never calls blocking `device.poll(Wait)` on the render thread: each slot transitions through
//! `Idle -> PendingMap` and is drained on a later frame via non-blocking `PollType::Poll`.

use core_ipc::ShmPreviewWriter;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const RING_SLOTS: usize = 3;

const MAP_NONE: u8 = 0;
const MAP_PENDING: u8 = 1;
const MAP_OK: u8 = 2;
const MAP_ERR: u8 = 3;

struct StagingSlot {
    buffer: wgpu::Buffer,
    status: Arc<AtomicU8>,
}

/// Asynchronous GPU texture readback ring writing tightly packed RGBA8 frames into shared memory.
pub struct PreviewReadbackRing {
    width: u32,
    height: u32,
    unpadded_bytes_per_row: u32,
    padded_bytes_per_row: u32,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    slots: Vec<StagingSlot>,
    next_submit: usize,
    packed_scratch: Vec<u8>,
    shm_writer: ShmPreviewWriter,
}

impl PreviewReadbackRing {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        shm_writer: ShmPreviewWriter,
    ) -> Self {
        let unpadded_bytes_per_row = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buffer_size = u64::from(padded_bytes_per_row) * u64::from(height);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preview_offscreen_rgba8"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut slots = Vec::with_capacity(RING_SLOTS);
        for idx in 0..RING_SLOTS {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("preview_staging_slot_{idx}")),
                size: buffer_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            slots.push(StagingSlot {
                buffer,
                status: Arc::new(AtomicU8::new(MAP_NONE)),
            });
        }

        Self {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
            texture,
            texture_view,
            slots,
            next_submit: 0,
            packed_scratch: vec![0u8; (unpadded_bytes_per_row * height) as usize],
            shm_writer,
        }
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.texture_view
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Drains any completed staging buffers without blocking and publishes the newest frame to SHM.
    pub fn drain_ready(&mut self, device: &wgpu::Device) -> u64 {
        let _ = device.poll(wgpu::PollType::Poll);
        let mut published = 0u64;

        for slot in &mut self.slots {
            let state = slot.status.load(Ordering::Acquire);
            if state == MAP_OK {
                let mut copied = false;
                if let Ok(mapped) = slot.buffer.slice(..).get_mapped_range() {
                    let src_stride = self.padded_bytes_per_row as usize;
                    let dst_stride = self.unpadded_bytes_per_row as usize;
                    for row in 0..self.height as usize {
                        let src_start = row * src_stride;
                        let dst_start = row * dst_stride;
                        self.packed_scratch[dst_start..dst_start + dst_stride]
                            .copy_from_slice(&mapped[src_start..src_start + dst_stride]);
                    }
                    copied = true;
                }
                slot.buffer.unmap();
                slot.status.store(MAP_NONE, Ordering::Release);

                if copied {
                    let ts_us = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_micros() as u64)
                        .unwrap_or(0);
                    if self
                        .shm_writer
                        .publish_rgba(&self.packed_scratch, ts_us)
                        .is_ok()
                    {
                        published += 1;
                    }
                }
            } else if state == MAP_ERR {
                slot.status.store(MAP_NONE, Ordering::Release);
            }
        }

        published
    }

    /// Records a GPU copy from the offscreen preview texture into the next idle staging slot.
    /// Returns the slot index if a copy was encoded, or `None` if all 3 slots are currently in flight.
    pub fn encode_copy(&mut self, encoder: &mut wgpu::CommandEncoder) -> Option<usize> {
        let slot_idx = self.next_submit;
        if self.slots[slot_idx].status.load(Ordering::Acquire) != MAP_NONE {
            return None;
        }

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.slots[slot_idx].buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        self.next_submit = (slot_idx + 1) % RING_SLOTS;
        Some(slot_idx)
    }

    /// Begins asynchronous CPU mapping for a slot immediately after `queue.submit(...)`.
    pub fn begin_map(&mut self, slot_idx: usize) {
        let slot = &self.slots[slot_idx];
        slot.status.store(MAP_PENDING, Ordering::Release);
        let status = Arc::clone(&slot.status);
        slot.buffer.slice(..).map_async(wgpu::MapMode::Read, move |res| {
            status.store(
                if res.is_ok() { MAP_OK } else { MAP_ERR },
                Ordering::Release,
            );
        });
    }
}
