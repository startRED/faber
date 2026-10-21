// The single-file tiny build has no unchecked imports. It is downloaded only inside an opened Live visual frame.
export const MERMAID_URL = 'https://cdn.jsdelivr.net/npm/@mermaid-js/tiny@12.2.0/dist/mermaid.tiny.js';
export const MERMAID_INTEGRITY = 'sha384-QCupLuRUDJe+dQ24RHpme+E1f5hpLkiB3P6YhujXLqc2Inrj/C+GgF8bgTuSXCiE';

export function mermaidDocument(source: string, notice = ''): string {
  const code = source.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
  return `<!doctype html><meta charset="utf-8"><style>body{margin:16px}pre{white-space:pre-wrap;overflow-wrap:anywhere}svg{max-width:100%;height:auto}*{animation:none!important;transition:none!important}</style>
<p id="fallback">${notice.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;')}</p><pre id="source">${code}</pre><div id="drawing"></div>
<script src="${MERMAID_URL}" integrity="${MERMAID_INTEGRITY}" crossorigin="anonymous"></script>
<script>
if (window.mermaid && document.getElementById('source').textContent.length <= 16384) {
  mermaid.initialize({startOnLoad:false,securityLevel:'strict',maxTextSize:16384,maxEdges:200,
    suppressErrorRendering:true,htmlLabels:false,flowchart:{htmlLabels:false},
    secure:['secure','securityLevel','startOnLoad','maxTextSize','maxEdges','suppressErrorRendering','htmlLabels','flowchart']});
  mermaid.render('diagram',document.getElementById('source').textContent).then(({svg})=>{
    document.getElementById('drawing').innerHTML=svg;
    document.getElementById('source').hidden=true;
    document.getElementById('fallback').hidden=true;
  }).catch(()=>{});
}
</script>`;
}
