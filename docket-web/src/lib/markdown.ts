/** Markdown as item bodies use it, rendered to HTML with every input character escaped first. */

const ID = /\b([A-Z]{1,3}\d+)\b/g;

export function escape(text: string): string {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

export interface Links {
  /** Whether an id names an item of the project. */
  known: (id: string) => boolean;
  /** Where an item's id links to. */
  item: (id: string) => string;
}

const NO_LINKS: Links = { known: () => false, item: () => '' };

/** Bold, italic, links and item ids in one line of escaped text; code spans are left as they are. */
function inline(text: string, links: Links): string {
  const parts = text.split(/(`[^`]+`)/g);
  return parts
    .map((part, i) => {
      if (i % 2 === 1) return `<code>${escape(part.slice(1, -1))}</code>`;
      let s = escape(part);
      s = s.replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noreferrer">$1</a>');
      s = s.replace(/(^|[\s(])(https?:\/\/[^\s<)]+)/g, '$1<a href="$2" target="_blank" rel="noreferrer">$2</a>');
      s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
      s = s.replace(/(^|[\s(])\*([^*\s][^*]*)\*/g, '$1<em>$2</em>');
      s = s.replace(/(<a [^>]*>.*?<\/a>)|\b([A-Z]{1,3}\d+)\b/g, (m, anchor: string | undefined, id: string | undefined) => {
        if (anchor || !id || !links.known(id)) return m;
        return `<a class="ref" href="${escape(links.item(id))}">${id}</a>`;
      });
      return s;
    })
    .join('');
}

/** Item bodies to HTML: paragraphs, headings, bullet and numbered lists, fenced code and quotes. */
export function render(source: string, links: Links = NO_LINKS): string {
  const lines = source.replace(/\r\n?/g, '\n').split('\n');
  const out: string[] = [];
  let para: string[] = [];
  let list: { tag: 'ul' | 'ol'; items: string[] } | null = null;
  const flushPara = () => {
    if (para.length) out.push(`<p>${para.map((l) => inline(l, links)).join('<br>')}</p>`);
    para = [];
  };
  const flushList = () => {
    if (list) out.push(`<${list.tag}>${list.items.map((i) => `<li>${inline(i, links)}</li>`).join('')}</${list.tag}>`);
    list = null;
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (/^\s*```/.test(line)) {
      flushPara();
      flushList();
      const code: string[] = [];
      while (++i < lines.length && !/^\s*```/.test(lines[i])) code.push(lines[i]);
      out.push(`<pre><code>${escape(code.join('\n'))}</code></pre>`);
      continue;
    }
    const heading = /^(#{1,4})\s+(.*)$/.exec(line);
    const bullet = /^\s*[-*]\s+(.*)$/.exec(line);
    const numbered = /^\s*\d+[.)]\s+(.*)$/.exec(line);
    const quote = /^>\s?(.*)$/.exec(line);
    if (line.trim() === '') {
      flushPara();
      flushList();
    } else if (heading) {
      flushPara();
      flushList();
      const level = Math.min(heading[1].length + 2, 6);
      out.push(`<h${level}>${inline(heading[2], links)}</h${level}>`);
    } else if (bullet || numbered) {
      flushPara();
      const tag = bullet ? 'ul' : 'ol';
      if (list && list.tag !== tag) flushList();
      list ??= { tag, items: [] };
      list.items.push((bullet ?? numbered)![1]);
    } else if (quote) {
      flushPara();
      flushList();
      out.push(`<blockquote>${inline(quote[1], links)}</blockquote>`);
    } else if (list && /^\s{2,}\S/.test(line)) {
      list.items[list.items.length - 1] += ` ${line.trim()}`;
    } else {
      flushList();
      para.push(line);
    }
  }
  flushPara();
  flushList();
  return out.join('\n');
}

/** The ids a text names, in order, each once. */
export function ids(text: string): string[] {
  return [...new Set([...text.matchAll(ID)].map((m) => m[1]))];
}
