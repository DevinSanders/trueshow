export interface GuideSection {
  id: string;
  title: string;
  plainText: string;
  html: string;
}

export interface ParsedUserGuide {
  title: string;
  introHtml: string;
  sections: GuideSection[];
}

export function slugifyHeading(text: string): string {
  return text
    .toLowerCase()
    .replace(/[`*_~]/g, "")
    .replace(/[^a-z0-9\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-");
}

function escapeHtml(raw: string): string {
  return raw
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function renderInline(raw: string, imageMap: Record<string, string>): string {
  // Preserve inline code spans before escaping & formatting surrounding text
  const codeSpans: string[] = [];
  const withPlaceholders = raw.replace(/`([^`]+)`/g, (_m, code: string) => {
    const idx = codeSpans.push(`<code>${escapeHtml(code)}</code>`) - 1;
    return `\u0000CODE${idx}\u0000`;
  });

  let out = escapeHtml(withPlaceholders);

  // Images: ![alt](src)
  out = out.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (_m, alt: string, src: string) => {
    const resolved = imageMap[src] ?? src;
    return `<figure class="ug-figure"><img src="${resolved}" alt="${alt}" data-lightbox-src="${resolved}" data-lightbox-alt="${alt}" loading="lazy" /><figcaption>${alt} — click to enlarge</figcaption></figure>`;
  });

  // Links: [label](#anchor) or [label](url)
  out = out.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_m, label: string, href: string) => {
    if (href.startsWith("#")) {
      const anchor = href.slice(1);
      return `<a href="${href}" data-guide-anchor="${anchor}">${label}</a>`;
    }
    return `<a href="${href}" target="_blank" rel="noopener noreferrer">${label}</a>`;
  });

  // Bold + Italic
  out = out.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  out = out.replace(/(^|[\s(])\*([^*]+)\*(?=[\s).,;:!?]|$)/g, "$1<em>$2</em>");

  // Restore inline code spans
  out = out.replace(/\u0000CODE(\d+)\u0000/g, (_m, idx: string) => codeSpans[Number(idx)] ?? "");
  return out;
}

function isTableSeparator(line: string): boolean {
  return /^\s*\|?(\s*:?-{3,}:?\s*\|)+\s*:?-{3,}:?\s*\|?\s*$/.test(line);
}

function splitTableRow(line: string): string[] {
  const trimmed = line.trim().replace(/^\|/, "").replace(/\|$/, "");
  return trimmed.split("|").map((cell) => cell.trim());
}

export function renderMarkdownBlock(md: string, imageMap: Record<string, string> = {}): string {
  const lines = md.replace(/\r\n/g, "\n").split("\n");
  const out: string[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];
    const trimmed = line.trim();

    if (trimmed === "") {
      i += 1;
      continue;
    }

    // Horizontal rule
    if (/^-{3,}$/.test(trimmed)) {
      out.push("<hr />");
      i += 1;
      continue;
    }

    // Fenced code block
    if (trimmed.startsWith("```")) {
      const codeLines: string[] = [];
      i += 1;
      while (i < lines.length && !lines[i].trim().startsWith("```")) {
        codeLines.push(lines[i]);
        i += 1;
      }
      if (i < lines.length) i += 1;
      out.push(`<pre class="ug-pre"><code>${escapeHtml(codeLines.join("\n"))}</code></pre>`);
      continue;
    }

    // Headings ### or ####
    const headingMatch = /^(#{2,4})\s+(.+)$/.exec(trimmed);
    if (headingMatch) {
      const level = headingMatch[1].length;
      const text = headingMatch[2].trim();
      const id = slugifyHeading(text);
      out.push(`<h${level} id="${id}">${renderInline(text, imageMap)}</h${level}>`);
      i += 1;
      continue;
    }

    // Blockquote
    if (trimmed.startsWith(">")) {
      const quoteLines: string[] = [];
      while (i < lines.length && lines[i].trim().startsWith(">")) {
        quoteLines.push(lines[i].trim().replace(/^>\s?/, ""));
        i += 1;
      }
      out.push(`<blockquote class="ug-quote">${renderInline(quoteLines.join(" "), imageMap)}</blockquote>`);
      continue;
    }

    // Markdown table
    if (trimmed.includes("|") && i + 1 < lines.length && isTableSeparator(lines[i + 1])) {
      const headers = splitTableRow(line);
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && lines[i].trim().includes("|") && lines[i].trim() !== "") {
        rows.push(splitTableRow(lines[i]));
        i += 1;
      }
      const thead = `<thead><tr>${headers.map((h) => `<th>${renderInline(h, imageMap)}</th>`).join("")}</tr></thead>`;
      const tbody = `<tbody>${rows
        .map((r) => `<tr>${r.map((c) => `<td>${renderInline(c, imageMap)}</td>`).join("")}</tr>`)
        .join("")}</tbody>`;
      out.push(`<div class="ug-table-wrap"><table class="ug-table">${thead}${tbody}</table></div>`);
      continue;
    }

    // Unordered list
    if (/^\s*[-*]\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*[-*]\s+/.test(lines[i])) {
        items.push(`<li>${renderInline(lines[i].replace(/^\s*[-*]\s+/, ""), imageMap)}</li>`);
        i += 1;
      }
      out.push(`<ul>${items.join("")}</ul>`);
      continue;
    }

    // Ordered list
    if (/^\s*\d+\.\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*\d+\.\s+/.test(lines[i])) {
        items.push(`<li>${renderInline(lines[i].replace(/^\s*\d+\.\s+/, ""), imageMap)}</li>`);
        i += 1;
      }
      out.push(`<ol>${items.join("")}</ol>`);
      continue;
    }

    // Standalone image line
    if (/^!\[[^\]]*\]\([^)]+\)$/.test(trimmed)) {
      out.push(renderInline(trimmed, imageMap));
      i += 1;
      continue;
    }

    // Paragraph (join contiguous plain text lines)
    const paraLines: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !/^(#{2,4}\s+|```|>|\s*[-*]\s+|\s*\d+\.\s+|-{3,}$|!\[)/.test(lines[i].trim()) &&
      !(lines[i].trim().includes("|") && i + 1 < lines.length && isTableSeparator(lines[i + 1]))
    ) {
      paraLines.push(lines[i].trim());
      i += 1;
    }
    if (paraLines.length > 0) {
      out.push(`<p>${renderInline(paraLines.join(" "), imageMap)}</p>`);
    } else {
      i += 1;
    }
  }

  return out.join("\n");
}

export function parseGuideSections(
  markdown: string,
  imageMap: Record<string, string> = {},
): ParsedUserGuide {
  const normalized = markdown.replace(/\r\n/g, "\n");
  const lines = normalized.split("\n");

  let title = "OpenPresenter User Guide";
  const introLines: string[] = [];
  const rawSections: { title: string; lines: string[] }[] = [];
  let current: { title: string; lines: string[] } | null = null;

  for (const line of lines) {
    if (!current && /^#\s+/.test(line)) {
      title = line.replace(/^#\s+/, "").trim();
      continue;
    }
    const h2 = /^##\s+(.+)$/.exec(line);
    if (h2) {
      current = { title: h2[1].trim(), lines: [] };
      rawSections.push(current);
      continue;
    }
    if (current) {
      current.lines.push(line);
    } else {
      introLines.push(line);
    }
  }

  const sections: GuideSection[] = rawSections
    .filter((sec) => sec.title.toLowerCase() !== "table of contents")
    .map((sec) => {
      const cleanTitle = sec.title.replace(/`/g, "");
      const bodyMd = sec.lines.join("\n");
      return {
        id: slugifyHeading(sec.title),
        title: cleanTitle,
        plainText: `${cleanTitle}\n${bodyMd}`.toLowerCase(),
        html: renderMarkdownBlock(bodyMd, imageMap),
      };
    });

  return {
    title,
    introHtml: renderMarkdownBlock(introLines.join("\n"), imageMap),
    sections,
  };
}
