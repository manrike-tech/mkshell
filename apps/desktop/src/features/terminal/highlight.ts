// Resaltado de coincidencias de búsqueda dentro del HTML congelado de un bloque.
// Solo se envuelven nodos de texto: el HTML de origen lo genera xterm.js y no
// se interpreta texto como marcado.

export function highlightHtml(html: string, query: string): string {
  const q = query.trim().toLowerCase();
  if (!q) return html;
  const tpl = document.createElement('template');
  tpl.innerHTML = html;
  const walker = document.createTreeWalker(tpl.content, NodeFilter.SHOW_TEXT);
  const nodes: Text[] = [];
  while (walker.nextNode()) nodes.push(walker.currentNode as Text);
  for (const node of nodes) {
    const text = node.data;
    const lower = text.toLowerCase();
    let idx = lower.indexOf(q);
    if (idx < 0) continue;
    const frag = document.createDocumentFragment();
    let last = 0;
    while (idx >= 0) {
      frag.append(text.slice(last, idx));
      const mark = document.createElement('mark');
      mark.textContent = text.slice(idx, idx + q.length);
      frag.append(mark);
      last = idx + q.length;
      idx = lower.indexOf(q, last);
    }
    frag.append(text.slice(last));
    node.replaceWith(frag);
  }
  return tpl.innerHTML;
}

/** Indica si un bloque coincide con la búsqueda. */
export function matches(query: string, ...texts: (string | null | undefined)[]): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return texts.some((t) => (t ?? '').toLowerCase().includes(q));
}
