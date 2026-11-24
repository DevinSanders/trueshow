<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy } from "svelte";
  import {
    allowDrop,
    clearAppDrag,
    readDroppedArrangementPayload,
    startAppDrag,
  } from "./dragStore";
  import type { Arrangement, AutoPlayTrack, RecordedCue, SceneState, Slide } from "./types";

  interface Props {
    scene: SceneState;
    editingTimerSlideId?: string | null;
    onSceneChange: (next: SceneState) => void;
    onTriggerSlide: (slideId: string) => void;
    onError: (msg: string) => void;
    onClearTimerTarget?: () => void;
  }

  let {
    scene,
    editingTimerSlideId = null,
    onSceneChange,
    onTriggerSlide,
    onError,
    onClearTimerTarget,
  }: Props = $props();

  let showArrangements = $state(false);
  let showAutoPlay = $state(false);
  let newArrangementName = $state("");
  let dragOverSlotIdx = $state<number | "end" | null>(null);

  // Per-slide timer editor state
  let timerSecsInput = $state(6.0);
  let timerLoopFirst = $state(false);
  let defaultFallbackSecs = $state(6.0);
  let loopPlayback = $state(true);
  let selectedTrackId = $state<string>("per-slide");
  let trackNameInput = $state("Recorded Cue Track");

  // Live Cue Recording state
  let recording = $state(false);
  let recordStartMs = $state(0);
  let recordElapsedMs = $state(0);
  let recordedEntries = $state<{ slideId: string; label: string; startedAtMs: number }[]>([]);
  let recordStatus = $state<string | null>(null);

  // Auto-Play Playback state
  let playing = $state(false);
  let countdownMs = $state(0);
  let playTrackCursor = $state(0);
  let tickHandle: ReturnType<typeof setInterval> | null = null;
  let lastRecordedSlideId: string | null = null;

  const masterSlides = $derived(scene.master_deck?.length ? scene.master_deck : scene.deck);
  const masterGroups = $derived.by(() => {
    const seen = new Set<string>();
    const out: string[] = [];
    for (const s of masterSlides) {
      if (!seen.has(s.group)) {
        seen.add(s.group);
        out.push(s.group);
      }
    }
    return out;
  });

  const arrangements = $derived<Arrangement[]>(scene.active_arrangements ?? []);
  const activeArrangementId = $derived<string | null>(scene.active_arrangement_id ?? null);
  const activeArrangement = $derived<Arrangement | null>(
    arrangements.find((a) => a.id === activeArrangementId) ?? null,
  );
  const cueTracks = $derived<AutoPlayTrack[]>(scene.active_cue_tracks ?? []);
  const activeTrack = $derived<AutoPlayTrack | null>(
    selectedTrackId === "per-slide"
      ? null
      : (cueTracks.find((t) => t.id === selectedTrackId) ?? null),
  );

  const timerTargetSlide = $derived<Slide | null>(
    (editingTimerSlideId ? scene.deck.find((s) => s.id === editingTimerSlideId) : null) ??
      scene.active_slide ??
      scene.deck[0] ??
      null,
  );

  // When parent right-clicks a slide -> "Set Auto-Play Timer...", open the panel & sync its current value.
  $effect(() => {
    if (editingTimerSlideId) {
      showAutoPlay = true;
      const target = scene.deck.find((s) => s.id === editingTimerSlideId);
      if (target) {
        timerSecsInput = target.auto_advance_secs ?? defaultFallbackSecs;
        timerLoopFirst = Boolean(target.loop_to_first);
      }
    }
  });

  // Automatically capture every slide trigger while Live Cue Recording is active.
  $effect(() => {
    if (!recording) return;
    const active = scene.active_slide;
    if (!active || active.id === lastRecordedSlideId) return;
    lastRecordedSlideId = active.id;
    const now = Date.now();
    recordedEntries = [
      ...recordedEntries,
      { slideId: active.id, label: active.label, startedAtMs: now },
    ];
  });

  function groupColor(group: string): string {
    const g = group.toLowerCase();
    if (g.includes("chorus")) return "#2563eb";
    if (g.includes("bridge")) return "#7c3aed";
    if (g.includes("verse")) return "#0d9488";
    if (g.includes("pre")) return "#d97706";
    if (g.includes("scripture")) return "#059669";
    return "#475569";
  }

  function formatDuration(ms: number): string {
    const totalTenths = Math.max(0, Math.floor(ms / 100));
    const tenths = totalTenths % 10;
    const totalSecs = Math.floor(totalTenths / 10);
    const mins = Math.floor(totalSecs / 60);
    const secs = totalSecs % 60;
    return `${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}.${tenths}`;
  }

  async function selectArrangement(arrId: string | null) {
    try {
      const next = await invoke<SceneState>("select_arrangement", { arrangementId: arrId });
      onSceneChange(next);
    } catch (err) {
      onError(`Switch arrangement failed: ${String(err)}`);
    }
  }

  async function persistArrangements(nextList: Arrangement[], nextActiveId: string | null) {
    try {
      const next = await invoke<SceneState>("save_arrangements", {
        arrangements: nextList,
        activeId: nextActiveId,
      });
      onSceneChange(next);
    } catch (err) {
      onError(`Save arrangement failed: ${String(err)}`);
    }
  }

  async function createArrangement(initialGroups?: string[]) {
    const name = newArrangementName.trim() || `Arrangement ${arrangements.length + 1}`;
    const arr: Arrangement = {
      id: `arr-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
      name,
      group_order: initialGroups ? [...initialGroups] : [...masterGroups],
    };
    newArrangementName = "";
    await persistArrangements([...arrangements, arr], arr.id);
  }

  async function duplicateActiveArrangement() {
    if (!activeArrangement) {
      await createArrangement([...masterGroups]);
      return;
    }
    const copy: Arrangement = {
      id: `arr-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
      name: `${activeArrangement.name} Copy`,
      group_order: [...activeArrangement.group_order],
    };
    await persistArrangements([...arrangements, copy], copy.id);
  }

  async function deleteActiveArrangement() {
    if (!activeArrangement) return;
    const remaining = arrangements.filter((a) => a.id !== activeArrangement.id);
    await persistArrangements(remaining, remaining[0]?.id ?? null);
  }

  async function mutateActiveOrder(mutate: (order: string[]) => string[]) {
    if (!activeArrangement) {
      // Auto-create a custom arrangement from Master when dropping directly onto Master!
      const base = mutate([...masterGroups]);
      const arr: Arrangement = {
        id: `arr-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
        name: "Custom Arrangement",
        group_order: base,
      };
      await persistArrangements([...arrangements, arr], arr.id);
      return;
    }
    const nextOrder = mutate([...activeArrangement.group_order]);
    const nextList = arrangements.map((a) =>
      a.id === activeArrangement.id ? { ...a, group_order: nextOrder } : a,
    );
    await persistArrangements(nextList, activeArrangement.id);
  }

  function handleDropOnSlot(event: DragEvent, targetIdx: number | "end") {
    dragOverSlotIdx = null;
    event.preventDefault();
    event.stopPropagation();
    const dropped = readDroppedArrangementPayload(event);
    if (!dropped) return;

    if (dropped.kind === "master-group") {
      void mutateActiveOrder((order) => {
        if (targetIdx === "end") {
          order.push(dropped.group);
        } else {
          order.splice(targetIdx, 0, dropped.group);
        }
        return order;
      });
    } else if (dropped.kind === "arrangement-slot") {
      void mutateActiveOrder((order) => {
        const from = dropped.fromIdx;
        if (from < 0 || from >= order.length) return order;
        const [moved] = order.splice(from, 1);
        if (targetIdx === "end") {
          order.push(moved);
        } else {
          const dest = targetIdx > from ? targetIdx - 1 : targetIdx;
          order.splice(Math.max(0, dest), 0, moved);
        }
        return order;
      });
    }
  }

  function stopTimersLoop() {
    if (tickHandle !== null) {
      clearInterval(tickHandle);
      tickHandle = null;
    }
  }

  function startRecording() {
    stopAutoPlay();
    recordStatus = null;
    const now = Date.now();
    recordStartMs = now;
    recordElapsedMs = 0;
    const first = scene.active_slide ?? scene.deck[0] ?? null;
    lastRecordedSlideId = first?.id ?? null;
    recordedEntries = first
      ? [{ slideId: first.id, label: first.label, startedAtMs: now }]
      : [];
    recording = true;
    stopTimersLoop();
    tickHandle = setInterval(() => {
      recordElapsedMs = Date.now() - recordStartMs;
    }, 100);
  }

  async function stopAndSaveRecording() {
    recording = false;
    stopTimersLoop();
    const endMs = Date.now();
    if (recordedEntries.length === 0) {
      recordStatus = "No slide cues were captured.";
      return;
    }

    const cues: RecordedCue[] = recordedEntries.map((entry, idx) => {
      const nextStart = recordedEntries[idx + 1]?.startedAtMs ?? endMs;
      const secs = Math.max(0.5, Math.round(((nextStart - entry.startedAtMs) / 1000) * 10) / 10);
      return {
        slide_id: entry.slideId,
        delay_secs: secs,
        label: entry.label,
      };
    });

    // Stamp per-slide timings using each slide's recorded dwell time (last occurrence wins for duplicates).
    const timingMap = new Map<string, { secs: number; loopFirst: boolean }>();
    cues.forEach((cue, i) => {
      timingMap.set(cue.slide_id, {
        secs: cue.delay_secs,
        loopFirst: loopPlayback && i === cues.length - 1,
      });
    });

    const timings = Array.from(timingMap.entries()).map(([slide_id, val]) => ({
      slide_id,
      auto_advance_secs: val.secs,
      loop_to_first: val.loopFirst,
    }));

    const track: AutoPlayTrack = {
      id: `track-${Date.now()}`,
      name: trackNameInput.trim() || `Recorded Cue Track ${cueTracks.length + 1}`,
      loop_playback: loopPlayback,
      cues,
    };

    try {
      const next = await invoke<SceneState>("apply_recorded_cues", {
        timings,
        cueTrack: track,
      });
      onSceneChange(next);
      selectedTrackId = track.id;
      const totalSecs = cues.reduce((acc, c) => acc + c.delay_secs, 0);
      recordStatus = `Saved "${track.name}" (${cues.length} cues · ${totalSecs.toFixed(1)}s)`;
    } catch (err) {
      onError(`Save recorded cues failed: ${String(err)}`);
    }
  }

  function resolveCurrentSlideDurationMs(): number {
    if (activeTrack && activeTrack.cues.length > 0) {
      const cue = activeTrack.cues[playTrackCursor];
      if (cue) return Math.max(200, Math.round(cue.delay_secs * 1000));
    }
    const cur = scene.active_slide;
    const secs = cur?.auto_advance_secs ?? defaultFallbackSecs;
    return Math.max(200, Math.round(Math.max(0.5, Number(secs)) * 1000));
  }

  function advanceAutoPlayStep() {
    if (activeTrack && activeTrack.cues.length > 0) {
      const nextIdx = playTrackCursor + 1;
      if (nextIdx >= activeTrack.cues.length) {
        if (loopPlayback || activeTrack.loop_playback) {
          playTrackCursor = 0;
        } else {
          stopAutoPlay();
          return;
        }
      } else {
        playTrackCursor = nextIdx;
      }
      const nextCue = activeTrack.cues[playTrackCursor];
      if (nextCue) {
        onTriggerSlide(nextCue.slide_id);
        countdownMs = Math.max(200, Math.round(nextCue.delay_secs * 1000));
      }
      return;
    }

    if (scene.deck.length === 0) {
      stopAutoPlay();
      return;
    }
    const curIdx = scene.active_slide
      ? scene.deck.findIndex((s) => s.id === scene.active_slide?.id)
      : -1;
    const curSlide = curIdx >= 0 ? scene.deck[curIdx] : null;
    const isLast = curIdx >= scene.deck.length - 1;
    if (isLast && !loopPlayback && !curSlide?.loop_to_first) {
      stopAutoPlay();
      return;
    }
    const nextIdx = (curIdx + 1) % scene.deck.length;
    const nextSlide = scene.deck[nextIdx];
    if (nextSlide) {
      onTriggerSlide(nextSlide.id);
      const nextSecs = nextSlide.auto_advance_secs ?? defaultFallbackSecs;
      countdownMs = Math.max(200, Math.round(Math.max(0.5, Number(nextSecs)) * 1000));
    }
  }

  function startAutoPlay() {
    if (recording) return;
    if (scene.deck.length === 0) {
      onError("Load a presentation before starting Auto-Play.");
      return;
    }
    playing = true;
    if (activeTrack && activeTrack.cues.length > 0) {
      playTrackCursor = 0;
      const firstCue = activeTrack.cues[0];
      onTriggerSlide(firstCue.slide_id);
      countdownMs = Math.max(200, Math.round(firstCue.delay_secs * 1000));
    } else {
      if (!scene.active_slide && scene.deck[0]) {
        onTriggerSlide(scene.deck[0].id);
      }
      countdownMs = resolveCurrentSlideDurationMs();
    }

    stopTimersLoop();
    tickHandle = setInterval(() => {
      countdownMs = Math.max(0, countdownMs - 100);
      if (countdownMs === 0) {
        advanceAutoPlayStep();
      }
    }, 100);
  }

  function stopAutoPlay() {
    playing = false;
    countdownMs = 0;
    stopTimersLoop();
  }

  async function saveSingleSlideTimer(clear = false) {
    if (!timerTargetSlide) return;
    const secs = clear ? null : Math.max(0.5, Number(timerSecsInput));
    try {
      const next = await invoke<SceneState>("set_slide_automation", {
        slideId: timerTargetSlide.id,
        autoAdvanceSecs: secs,
        loopToFirst: clear ? false : timerLoopFirst,
        actions: [],
      });
      onSceneChange(next);
      recordStatus = clear
        ? `Cleared timer on "${timerTargetSlide.label}"`
        : `Set ${secs?.toFixed(1)}s cue timer on "${timerTargetSlide.label}"`;
      onClearTimerTarget?.();
    } catch (err) {
      onError(`Save slide timer failed: ${String(err)}`);
    }
  }

  async function clearAllSlideTimers() {
    const timings = scene.deck.map((s) => ({
      slide_id: s.id,
      auto_advance_secs: null,
      loop_to_first: false,
    }));
    try {
      const next = await invoke<SceneState>("apply_recorded_cues", {
        timings,
        cueTrack: null,
      });
      onSceneChange(next);
      recordStatus = "Cleared auto-advance timers from all slides in deck.";
    } catch (err) {
      onError(`Clear timers failed: ${String(err)}`);
    }
  }

  async function removeSelectedCueTrack() {
    if (!activeTrack) return;
    try {
      const next = await invoke<SceneState>("delete_cue_track", { trackId: activeTrack.id });
      selectedTrackId = "per-slide";
      onSceneChange(next);
      recordStatus = `Deleted cue track "${activeTrack.name}"`;
    } catch (err) {
      onError(`Delete cue track failed: ${String(err)}`);
    }
  }

  onDestroy(() => {
    stopTimersLoop();
  });
</script>

<div class="arr-cues-shell" data-testid="arrangements-cues-bar">
  <div class="strip-bar">
    <div class="strip-left">
      <label class="arr-select-wrap">
        <span>Arrangement:</span>
        <select
          aria-label="Active Arrangement"
          value={activeArrangementId ?? ""}
          onchange={(e) => void selectArrangement(e.currentTarget.value || null)}
        >
          <option value="">Master ({masterSlides.length} slides)</option>
          {#each arrangements as arr (arr.id)}
            <option value={arr.id}>{arr.name} ({arr.group_order.length} sections)</option>
          {/each}
        </select>
      </label>

      <button
        type="button"
        class="chip-btn"
        class:active={showArrangements}
        onclick={() => (showArrangements = !showArrangements)}
      >
        Arrangements Editor
      </button>
      <button
        type="button"
        class="chip-btn"
        class:active={showAutoPlay}
        onclick={() => (showAutoPlay = !showAutoPlay)}
      >
        Auto-Play & Cue Recorder
      </button>
    </div>

    <div class="strip-right">
      {#if recording}
        <span class="rec-pill" data-testid="rec-indicator">
          ● REC {formatDuration(recordElapsedMs)} ({recordedEntries.length} cues)
        </span>
        <button type="button" class="chip-btn danger-solid" onclick={() => void stopAndSaveRecording()}>
          ■ Stop & Save Rec
        </button>
      {:else if playing}
        <span class="play-pill" data-testid="autoplay-indicator">
          ▶ AUTO · Next in {(countdownMs / 1000).toFixed(1)}s
        </span>
        <button type="button" class="chip-btn danger" onclick={stopAutoPlay}>■ Stop Auto-Play</button>
      {:else}
        <button type="button" class="chip-btn rec" onclick={startRecording}>● Record Cues</button>
        <button type="button" class="chip-btn play" onclick={startAutoPlay}>▶ Auto-Play</button>
      {/if}
    </div>
  </div>

  {#if recordStatus}
    <div class="mini-status" role="status">{recordStatus}</div>
  {/if}

  {#if showArrangements}
    <div class="subpanel" data-testid="arrangement-editor">
      <div class="sub-row head">
        <div class="row-title">
          <strong>Master Groups</strong>
          <span class="hint">Drag any group (or Master slide card) into the Arrangement Track below to copy/repeat, or click to append</span>
        </div>
        <div class="arr-actions">
          <input
            aria-label="New Arrangement Name"
            placeholder="New arrangement name..."
            bind:value={newArrangementName}
          />
          <button type="button" class="chip-btn primary" onclick={() => void createArrangement()}>
            + New
          </button>
          <button type="button" class="chip-btn" onclick={() => void duplicateActiveArrangement()}>
            Duplicate
          </button>
          {#if activeArrangement}
            <button type="button" class="chip-btn danger" onclick={() => void deleteActiveArrangement()}>
              Delete
            </button>
          {/if}
        </div>
      </div>

      <div class="token-row master-palette" data-testid="master-group-palette">
        {#each masterGroups as group (group)}
          <button
            type="button"
            class="group-token"
            draggable="true"
            style={`background:${groupColor(group)};`}
            ondragstart={(e) => startAppDrag(e, { type: "master-group", group })}
            ondragend={clearAppDrag}
            onclick={() => void mutateActiveOrder((order) => [...order, group])}
            title={`Drag "${group}" into the arrangement track or click to append`}
          >
            {group}
          </button>
        {/each}
      </div>

      <div class="row-title">
        <strong>
          {activeArrangement
            ? `Arrangement Track: ${activeArrangement.name}`
            : "Master Order (Drop a group below to create a Custom Arrangement)"}
        </strong>
      </div>

      <div
        class="token-row track-dropzone"
        class:drag-over={dragOverSlotIdx === "end"}
        role="group"
        aria-label="Arrangement sequence dropzone"
        data-testid="arrangement-dropzone"
        ondragenter={(e) => {
          if (allowDrop(e, ["master-group", "arrangement-slot"], "copy")) dragOverSlotIdx = "end";
        }}
        ondragover={(e) => {
          if (allowDrop(e, ["master-group", "arrangement-slot"], "copy")) dragOverSlotIdx = "end";
        }}
        ondragleave={() => {
          if (dragOverSlotIdx === "end") dragOverSlotIdx = null;
        }}
        ondrop={(e) => handleDropOnSlot(e, "end")}
      >
        {#each activeArrangement ? activeArrangement.group_order : masterGroups as group, idx (`${group}-${idx}`)}
          <div
            class="track-token"
            class:drop-before={dragOverSlotIdx === idx}
            style={`background:${groupColor(group)};`}
            role="group"
            draggable={Boolean(activeArrangement)}
            ondragstart={(e) => {
              if (activeArrangement) startAppDrag(e, { type: "arrangement-slot", fromIdx: idx });
            }}
            ondragend={clearAppDrag}
            ondragenter={(e) => {
              e.stopPropagation();
              if (allowDrop(e, ["master-group", "arrangement-slot"], "move")) dragOverSlotIdx = idx;
            }}
            ondragover={(e) => {
              e.stopPropagation();
              if (allowDrop(e, ["master-group", "arrangement-slot"], "move")) dragOverSlotIdx = idx;
            }}
            ondragleave={() => {
              if (dragOverSlotIdx === idx) dragOverSlotIdx = null;
            }}
            ondrop={(e) => handleDropOnSlot(e, idx)}
          >
            <span class="tok-idx">{idx + 1}</span>
            <span>{group}</span>
            {#if activeArrangement}
              <button
                type="button"
                class="tok-del"
                aria-label={`Remove ${group} at slot ${idx + 1}`}
                onclick={() => void mutateActiveOrder((order) => order.filter((_, i) => i !== idx))}
              >
                ×
              </button>
            {/if}
          </div>
        {/each}
        <span class="drop-hint">+ Drop group here</span>
      </div>
    </div>
  {/if}

  {#if showAutoPlay}
    <div class="subpanel" data-testid="autoplay-panel">
      <div class="sub-row">
        <label class="inline-field">
          <span>Playback Source:</span>
          <select aria-label="Auto-Play Cue Source" bind:value={selectedTrackId}>
            <option value="per-slide">Per-Slide Timers</option>
            {#each cueTracks as t (t.id)}
              <option value={t.id}>{t.name} ({t.cues.length} cues)</option>
            {/each}
          </select>
        </label>
        {#if activeTrack}
          <button type="button" class="chip-btn danger" onclick={() => void removeSelectedCueTrack()}>
            Delete Track
          </button>
        {/if}
        <label class="inline-field">
          <span>Fallback (s):</span>
          <input
            aria-label="Fallback Auto-Play Seconds"
            type="number"
            min="0.5"
            step="0.5"
            bind:value={defaultFallbackSecs}
            style="width:62px;"
          />
        </label>
        <label class="inline-field">
          <input type="checkbox" bind:checked={loopPlayback} />
          <span>Loop</span>
        </label>
        <label class="inline-field">
          <span>Rec Track Name:</span>
          <input
            aria-label="Recorded Track Name"
            bind:value={trackNameInput}
            style="width:150px;"
          />
        </label>
      </div>

      {#if timerTargetSlide}
        <div class="sub-row slide-timer-row">
          <span class="target-label">
            Slide Cue Timer: <strong>{timerTargetSlide.label}</strong>
          </span>
          <label class="inline-field">
            <span>Go-to-Next (s):</span>
            <input
              aria-label="Slide Auto-Advance Seconds"
              type="number"
              min="0.5"
              step="0.5"
              bind:value={timerSecsInput}
              style="width:68px;"
            />
          </label>
          <label class="inline-field">
            <input type="checkbox" bind:checked={timerLoopFirst} />
            <span>Loop to Slide 1</span>
          </label>
          <button type="button" class="chip-btn primary" onclick={() => void saveSingleSlideTimer(false)}>
            Save Slide Timer
          </button>
          <button type="button" class="chip-btn" onclick={() => void saveSingleSlideTimer(true)}>
            Clear Slide Timer
          </button>
          <button type="button" class="chip-btn danger" onclick={() => void clearAllSlideTimers()}>
            Clear All Timers
          </button>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .arr-cues-shell {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--bg-1);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 6px 10px;
    font-size: 12px;
  }

  .strip-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }

  .strip-left,
  .strip-right,
  .arr-actions,
  .sub-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .sub-row.head {
    justify-content: space-between;
  }

  .arr-select-wrap,
  .inline-field {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--text-1);
    font-size: 11px;
  }

  select,
  input[type="number"],
  input:not([type="checkbox"]) {
    background: var(--bg-0);
    color: var(--text-0);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 6px;
    font-size: 11px;
  }

  .chip-btn {
    padding: 4px 8px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-0);
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }

  .chip-btn.active {
    border-color: var(--accent);
    background: rgba(59, 130, 246, 0.2);
  }

  .chip-btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .chip-btn.rec {
    border-color: rgba(239, 68, 68, 0.55);
    color: #fca5a5;
  }

  .chip-btn.play {
    border-color: rgba(34, 197, 94, 0.55);
    color: #bbf7d0;
  }

  .chip-btn.danger {
    border-color: rgba(239, 68, 68, 0.5);
    color: #fca5a5;
  }

  .chip-btn.danger-solid {
    background: #b91c1c;
    border-color: #ef4444;
    color: #fff;
  }

  .rec-pill {
    padding: 3px 8px;
    border-radius: 999px;
    background: rgba(239, 68, 68, 0.22);
    border: 1px solid #ef4444;
    color: #fecaca;
    font-size: 11px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .play-pill {
    padding: 3px 8px;
    border-radius: 999px;
    background: rgba(34, 197, 94, 0.2);
    border: 1px solid var(--ok);
    color: #bbf7d0;
    font-size: 11px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .mini-status {
    padding: 4px 8px;
    border-radius: 5px;
    background: rgba(34, 197, 94, 0.14);
    color: #bbf7d0;
    font-size: 11px;
  }

  .subpanel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 6px;
    border-top: 1px solid var(--border);
  }

  .row-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
  }

  .hint {
    color: var(--text-1);
    font-size: 11px;
  }

  .token-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    padding: 6px;
    border-radius: 6px;
    background: var(--bg-0);
    border: 1px solid var(--border);
    min-height: 36px;
  }

  .track-dropzone.drag-over {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .group-token,
  .track-token {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 9px;
    border-radius: 5px;
    border: 1px solid rgba(255, 255, 255, 0.22);
    color: #fff;
    font-size: 11px;
    font-weight: 700;
    cursor: grab;
    user-select: none;
  }

  .track-token.drop-before {
    box-shadow: -3px 0 0 0 #38bdf8;
  }

  .tok-idx {
    font-size: 10px;
    opacity: 0.78;
  }

  .tok-del {
    border: none;
    background: rgba(0, 0, 0, 0.35);
    color: #fff;
    border-radius: 3px;
    padding: 0 4px;
    font-size: 11px;
    line-height: 1.3;
  }

  .drop-hint {
    font-size: 11px;
    color: var(--text-1);
    opacity: 0.7;
    margin-left: 4px;
  }

  .slide-timer-row {
    padding-top: 6px;
    border-top: 1px dashed var(--border);
  }

  .target-label {
    font-size: 11px;
    color: var(--text-1);
  }

  .target-label strong {
    color: var(--text-0);
  }
</style>
