// Drawing on a still of the page. The still and the canvas are the
// workspace's own, so what is drawn never meets the page's scripts.
await (async () => {
  const start = await dioxus.recv();
  // The script starts as the component is made, before its nodes are drawn.
  let base = null;
  let canvas = null;
  for (let frame = 0; frame < 120 && !(base && canvas); frame++) {
    base = document.getElementById('browser-markup-base-' + start.window);
    canvas = document.getElementById('browser-markup-canvas-' + start.window);
    if (!(base && canvas)) await new Promise((resolve) => requestAnimationFrame(resolve));
  }
  if (!base || !canvas) return;
  const stage = canvas.parentElement;
  const ctx = canvas.getContext('2d');
  const FONT = 'system-ui, -apple-system, "Segoe UI", sans-serif';
  const state = { tool: 'pen', color: '#ef4444', width: 4, font: 18 };
  let shapes = [];
  let past = [];
  let future = [];
  let drawing = null;
  let text = null;
  let id = 0;

  const tell = () => dioxus.send({ undo: past.length > 0, redo: future.length > 0, any: shapes.length > 0 });
  const commit = (next) => { past.push(shapes); shapes = next; future = []; tell(); };
  const point = (event) => {
    const r = canvas.getBoundingClientRect();
    return { x: event.clientX - r.left, y: event.clientY - r.top };
  };

  const stroke = (c, shape, k) => {
    c.save();
    c.lineCap = 'round';
    c.lineJoin = 'round';
    c.strokeStyle = shape.color;
    c.fillStyle = shape.color;
    c.lineWidth = shape.width * k;
    switch (shape.kind) {
      case 'pen':
      case 'highlight': {
        if (shape.kind === 'highlight') { c.globalAlpha = 0.35; c.lineWidth = shape.width * 4 * k; }
        const [first, ...rest] = shape.points;
        if (rest.length === 0) {
          c.beginPath();
          c.arc(first.x * k, first.y * k, Math.max(c.lineWidth / 2, 1), 0, Math.PI * 2);
          c.fill();
        } else {
          c.beginPath();
          c.moveTo(first.x * k, first.y * k);
          for (const p of rest) c.lineTo(p.x * k, p.y * k);
          c.stroke();
        }
        break;
      }
      case 'arrow': {
        const dx = shape.to.x - shape.from.x;
        const dy = shape.to.y - shape.from.y;
        c.beginPath();
        c.moveTo(shape.from.x * k, shape.from.y * k);
        c.lineTo(shape.to.x * k, shape.to.y * k);
        c.stroke();
        if (dx !== 0 || dy !== 0) {
          const angle = Math.atan2(dy, dx);
          const size = Math.max(10, shape.width * 3.5) * k;
          c.beginPath();
          c.moveTo(shape.to.x * k, shape.to.y * k);
          for (const turn of [Math.PI - 0.45, Math.PI + 0.45]) {
            c.lineTo(shape.to.x * k + size * Math.cos(angle + turn), shape.to.y * k + size * Math.sin(angle + turn));
            c.moveTo(shape.to.x * k, shape.to.y * k);
          }
          c.stroke();
        }
        break;
      }
      case 'rect': {
        const x = Math.min(shape.from.x, shape.to.x) * k;
        const y = Math.min(shape.from.y, shape.to.y) * k;
        c.strokeRect(x, y, Math.abs(shape.to.x - shape.from.x) * k, Math.abs(shape.to.y - shape.from.y) * k);
        break;
      }
      case 'ellipse': {
        c.beginPath();
        c.ellipse((shape.from.x + shape.to.x) / 2 * k, (shape.from.y + shape.to.y) / 2 * k,
          Math.abs(shape.to.x - shape.from.x) / 2 * k, Math.abs(shape.to.y - shape.from.y) / 2 * k, 0, 0, Math.PI * 2);
        c.stroke();
        break;
      }
      case 'text': {
        c.font = '600 ' + shape.size * k + 'px ' + FONT;
        c.textBaseline = 'top';
        c.lineJoin = 'round';
        c.lineWidth = Math.max(shape.size / 6, 2) * k;
        c.strokeStyle = shape.color.toLowerCase() === '#ffffff' ? 'rgba(0,0,0,.7)' : 'rgba(255,255,255,.9)';
        c.strokeText(shape.text, shape.at.x * k, shape.at.y * k);
        c.fillText(shape.text, shape.at.x * k, shape.at.y * k);
        break;
      }
    }
    c.restore();
  };

  const draw = () => {
    const k = canvas.width / Math.max(canvas.clientWidth, 1);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    for (const shape of drawing ? [...shapes, drawing] : shapes) stroke(ctx, shape, k);
  };

  const fit = () => {
    const ratio = window.devicePixelRatio || 1;
    canvas.width = Math.max(1, Math.round(stage.clientWidth * ratio));
    canvas.height = Math.max(1, Math.round(stage.clientHeight * ratio));
    draw();
  };
  new ResizeObserver(fit).observe(stage);
  fit();
  tell();

  const near = (p, a, b, tolerance) => {
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const length = dx * dx + dy * dy;
    const t = length === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / length));
    return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy)) <= tolerance;
  };
  const hits = (shape, p) => {
    const tolerance = Math.max(shape.width || 0, 8) + 4;
    switch (shape.kind) {
      case 'pen':
      case 'highlight':
        return shape.points.length === 1
          ? Math.hypot(p.x - shape.points[0].x, p.y - shape.points[0].y) <= tolerance
          : shape.points.some((q, at) => at > 0 && near(p, shape.points[at - 1], q, tolerance));
      case 'arrow':
        return near(p, shape.from, shape.to, tolerance);
      case 'rect':
      case 'ellipse': {
        const x = Math.min(shape.from.x, shape.to.x);
        const y = Math.min(shape.from.y, shape.to.y);
        const w = Math.abs(shape.to.x - shape.from.x);
        const h = Math.abs(shape.to.y - shape.from.y);
        return p.x >= x - tolerance && p.x <= x + w + tolerance && p.y >= y - tolerance && p.y <= y + h + tolerance;
      }
      case 'text': {
        ctx.save();
        ctx.font = '600 ' + shape.size + 'px ' + FONT;
        const w = ctx.measureText(shape.text).width / (canvas.width / Math.max(canvas.clientWidth, 1));
        ctx.restore();
        return p.x >= shape.at.x - 4 && p.x <= shape.at.x + w + 4 && p.y >= shape.at.y - 4 && p.y <= shape.at.y + shape.size + 4;
      }
    }
    return false;
  };

  const finishText = (keep) => {
    if (!text) return;
    const { input, at, color, size } = text;
    text = null;
    const value = keep ? input.value.trim() : '';
    input.remove();
    if (value) commit([...shapes, { kind: 'text', id: ++id, color, size, at, text: value }]);
    draw();
  };

  canvas.addEventListener('pointerdown', (event) => {
    if (event.button !== 0) return;
    if (text) { finishText(true); return; }
    const p = point(event);
    try { canvas.setPointerCapture(event.pointerId); } catch (_) { /* a pointer that is not held cannot be captured */ }
    switch (state.tool) {
      case 'pen':
      case 'highlight':
        drawing = { kind: state.tool, id: ++id, color: state.color, width: state.width, points: [p] };
        break;
      case 'arrow':
      case 'rect':
      case 'ellipse':
        drawing = { kind: state.tool, id: ++id, color: state.color, width: state.width, from: p, to: p };
        break;
      case 'eraser': {
        const index = shapes.findLastIndex((shape) => hits(shape, p));
        if (index >= 0) commit(shapes.filter((_, at) => at !== index));
        draw();
        break;
      }
      case 'text': {
        const input = document.createElement('input');
        input.setAttribute('aria-label', 'Annotation text');
        input.className = 'browser-markup-text';
        const size = state.font;
        const color = state.color;
        Object.assign(input.style, {
          left: p.x + 'px', top: p.y + 'px', color, fontSize: size + 'px',
          textShadow: color.toLowerCase() === '#ffffff' ? '0 0 3px rgba(0,0,0,.7)' : '0 0 3px rgba(255,255,255,.9)',
        });
        input.addEventListener('pointerdown', (e) => e.stopPropagation());
        input.addEventListener('keydown', (e) => {
          e.stopPropagation();
          if (e.key === 'Enter' && !e.isComposing) { e.preventDefault(); finishText(true); }
          else if (e.key === 'Escape') { e.preventDefault(); finishText(false); }
        });
        input.addEventListener('blur', () => finishText(true));
        stage.append(input);
        text = { input, at: p, color, size };
        requestAnimationFrame(() => input.focus());
        event.preventDefault();
        break;
      }
    }
  });
  canvas.addEventListener('pointermove', (event) => {
    if (!drawing) return;
    const p = point(event);
    if (drawing.points) drawing.points.push(p); else drawing.to = p;
    draw();
  });
  const release = () => {
    if (!drawing) return;
    const shape = drawing;
    drawing = null;
    commit([...shapes, shape]);
    draw();
  };
  canvas.addEventListener('pointerup', release);
  canvas.addEventListener('pointercancel', release);
  canvas.addEventListener('lostpointercapture', release);

  const exported = () => {
    const out = document.createElement('canvas');
    out.width = base.naturalWidth;
    out.height = base.naturalHeight;
    const c = out.getContext('2d');
    c.drawImage(base, 0, 0);
    const k = out.width / Math.max(canvas.clientWidth, 1);
    for (const shape of shapes) stroke(c, shape, k);
    return out.toDataURL('image/png').split(',')[1];
  };

  for (;;) {
    const message = await dioxus.recv();
    if (message.stop) { finishText(false); return; }
    if (message.tool !== undefined) { finishText(true); state.tool = message.tool; }
    if (message.color !== undefined) state.color = message.color;
    if (message.width !== undefined) state.width = message.width;
    if (message.font !== undefined) state.font = message.font;
    if (message.undo) { finishText(true); const prev = past.pop(); if (prev) { future.unshift(shapes); shapes = prev; tell(); draw(); } }
    if (message.redo) { finishText(true); const next = future.shift(); if (next) { past.push(shapes); shapes = next; tell(); draw(); } }
    if (message.clear && shapes.length) { finishText(false); commit([]); draw(); }
    if (message.export) { finishText(true); dioxus.send({ png: exported() }); }
  }
})();
