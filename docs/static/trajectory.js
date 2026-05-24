// Trajectory viewer — fetches trajectory.jsonl from the same directory and
// renders one row per event. Tool calls are collapsed by default; click to
// expand the args and output. Keeps everything client-side so we don't have
// to generate per-event HTML server-side.

(function () {
  const eventsEl = document.getElementById('events');
  const countEl = document.getElementById('event-count');
  const promptEl = document.getElementById('prompt-block');

  // Load the system prompt
  fetch('prompt.md')
    .then(r => r.ok ? r.text() : '(no prompt.md)')
    .then(t => { promptEl.textContent = t; })
    .catch(() => { promptEl.textContent = '(failed to load)'; });

  // Load the trajectory
  fetch('trajectory.jsonl')
    .then(r => {
      if (!r.ok) throw new Error('HTTP ' + r.status);
      return r.text();
    })
    .then(renderTrajectory)
    .catch(err => {
      eventsEl.innerHTML = '<p class="post-meta">failed to load trajectory: ' + escapeHTML(err.message) + '</p>';
    });

  function renderTrajectory(text) {
    const lines = text.split('\n').filter(l => l.trim());
    const events = [];
    for (const line of lines) {
      try {
        events.push(JSON.parse(line));
      } catch (e) {
        // skip malformed lines
      }
    }
    countEl.textContent = events.length + ' events';

    eventsEl.innerHTML = '';
    let stepIdx = 0;
    for (const e of events) {
      const row = renderEvent(e, stepIdx);
      if (row) eventsEl.appendChild(row);
      if (e.type === 'step_start') stepIdx++;
    }
    wireFilters();
  }

  function renderEvent(e, stepIdx) {
    const wrap = document.createElement('div');
    wrap.className = 'event event-' + (e.type || 'unknown');

    switch (e.type) {
      case 'step_start':
        wrap.classList.add('event-boundary');
        wrap.innerHTML = '<div class="step-sep">step ' + (stepIdx + 1) + '</div>';
        return wrap;

      case 'step_finish': {
        wrap.classList.add('event-boundary');
        const p = e.part || {};
        const tok = p.tokens || {};
        const cost = (p.cost !== undefined) ? '$' + Number(p.cost).toFixed(4) : '';
        const total = (tok.input || 0) + (tok.output || 0) + (tok.reasoning || 0);
        const reason = p.reason || '';
        const bits = [];
        if (reason) bits.push(reason);
        if (total) bits.push(humanTokens(total) + ' tok');
        if (cost) bits.push(cost);
        wrap.innerHTML = '<div class="step-finish">↳ ' + escapeHTML(bits.join(' · ')) + '</div>';
        return wrap;
      }

      case 'tool_use': {
        const p = e.part || {};
        const tool = p.tool || '?';
        const state = p.state || {};
        const input = state.input || {};
        const output = state.output;
        const oneLine = summarizeInput(tool, input);

        const det = document.createElement('details');
        det.className = 'tool-use';
        det.innerHTML =
          '<summary><span class="tool-name">' + escapeHTML(tool) + '</span> ' +
          '<span class="tool-summary">' + escapeHTML(oneLine) + '</span></summary>';
        if (Object.keys(input).length) {
          const argsBlock = document.createElement('div');
          argsBlock.className = 'tool-section';
          argsBlock.innerHTML = '<div class="tool-label">input</div><pre>' + escapeHTML(JSON.stringify(input, null, 2)) + '</pre>';
          det.appendChild(argsBlock);
        }
        if (output !== undefined && output !== null && output !== '') {
          const outBlock = document.createElement('div');
          outBlock.className = 'tool-section';
          const outText = typeof output === 'string' ? output : JSON.stringify(output, null, 2);
          outBlock.innerHTML = '<div class="tool-label">output</div><pre>' + escapeHTML(outText) + '</pre>';
          det.appendChild(outBlock);
        }
        wrap.appendChild(det);
        return wrap;
      }

      case 'text': {
        const p = e.part || {};
        const text = p.text || '';
        if (!text.trim()) return null;
        const block = document.createElement('div');
        block.className = 'assistant-text';
        block.textContent = text;
        wrap.appendChild(block);
        return wrap;
      }

      default:
        return null;
    }
  }

  function summarizeInput(tool, input) {
    // Make one-line summaries for the common tool shapes
    if (tool === 'bash' && input.command) {
      return truncate(input.command, 140);
    }
    if ((tool === 'write' || tool === 'edit') && input.filePath) {
      return input.filePath;
    }
    if (tool === 'read' && input.filePath) {
      return input.filePath;
    }
    if (tool === 'webfetch' && input.url) {
      return input.url;
    }
    if (tool === 'todowrite' && Array.isArray(input.todos)) {
      return input.todos.length + ' todos';
    }
    return truncate(JSON.stringify(input), 140);
  }

  function truncate(s, n) {
    if (s.length <= n) return s;
    return s.slice(0, n - 1) + '…';
  }

  function humanTokens(n) {
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
    if (n >= 1_000) return (n / 1_000).toFixed(1) + 'k';
    return String(n);
  }

  function escapeHTML(s) {
    return String(s)
      .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
  }

  function wireFilters() {
    const toolEl = document.getElementById('filter-tool-use');
    const textEl = document.getElementById('filter-text');
    const stepEl = document.getElementById('filter-step');
    function apply() {
      eventsEl.classList.toggle('hide-tool', !toolEl.checked);
      eventsEl.classList.toggle('hide-text', !textEl.checked);
      eventsEl.classList.toggle('hide-step', !stepEl.checked);
    }
    toolEl.addEventListener('change', apply);
    textEl.addEventListener('change', apply);
    stepEl.addEventListener('change', apply);
  }
})();
