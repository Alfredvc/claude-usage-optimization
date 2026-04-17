// @ts-nocheck
/* Verbatim port of legacy inline React app. Phase 2 of migration — refactor later. */
import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import * as Recharts from 'recharts';
import ReactDOM from 'react-dom/client';

// Shim: legacy code destructures `window.Recharts` in several local scopes.
(window as any).Recharts = Recharts;
(window as any).ReactDOM = ReactDOM;

const { BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer, AreaChart, Area, ReferenceArea } = window.Recharts || {};

// ── Pricing (USD / million tokens) — used for cost bar color breakdown only ──
const PRICES = {
  opus:   [15.00, 1.50, 18.75, 75.00],
  sonnet: [3.00,  0.30,  3.75, 15.00],
  haiku:  [0.80,  0.08,  1.00,  4.00],
};

function modelTier(model = '') {
  const m = model.toLowerCase();
  if (m.includes('opus'))  return PRICES.opus;
  if (m.includes('haiku')) return PRICES.haiku;
  return PRICES.sonnet;
}

function computeCost(usage, model = '') {
  const p = modelTier(model);
  const { input, cr, cw, out } = usage;
  const costInput = input * p[0] / 1e6;
  const costCr    = cr    * p[1] / 1e6;
  const costCw    = cw    * p[2] / 1e6;
  const costOut   = out   * p[3] / 1e6;
  return { costInput, costCr, costCw, costOut, total: costInput + costCr + costCw + costOut };
}

// ── Extract usage from flat DB fields ─────────────────────────────────────────
function getUsage(item) {
  const input = item.input_tokens || 0;
  const cr    = item.cache_read_input_tokens || 0;
  const cw    = item.cache_creation_input_tokens || 0;
  const out   = item.output_tokens || 0;
  return { input, cr, cw, out, total: input + cr + cw + out };
}

// ── Formatting ─────────────────────────────────────────────────────────────
const fmtTok  = n => n >= 1e6 ? `${(n/1e6).toFixed(2)}M` : n >= 1000 ? `${(n/1000).toFixed(1)}K` : String(n);
const fmtDate = ts => ts ? new Date(ts).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }) : '?';

function fmtCost(c) {
  if (!c || c === 0) return '$0.00';
  if (c >= 1)    return `$${c.toFixed(2)}`;
  if (c >= 0.01) return `$${c.toFixed(3)}`;
  return `$${c.toFixed(4)}`;
}

const fmtPct = n => `${((n || 0) * 100).toFixed(1)}%`;
const fmtNum = n => n >= 1e6 ? `${(n/1e6).toFixed(2)}M` : n >= 1000 ? `${(n/1000).toFixed(1)}K` : String(n || 0);
const shortModel = m => (m || '').replace('claude-', '').replace(/-\d{8}$/, '') || 'unknown';
const shortPath = (p, n=3) => {
  if (!p) return '';
  const parts = p.split('/').filter(Boolean);
  return parts.length > n ? '…/' + parts.slice(-n).join('/') : p;
};

// ── TruncText ──────────────────────────────────────────────────────────────
function TruncText({ text, limit = 200, className = '' }) {
  const [expanded, setExpanded] = useState(false);
  if (!text) return null;
  const short = text.length > limit;
  return (
    <span className={className}>
      {(!short || expanded) ? text : text.slice(0, limit) + '…'}
      {short && <button className="expand-btn" onClick={() => setExpanded(e => !e)}>{expanded ? ' less' : ' more'}</button>}
    </span>
  );
}

// ── Token + Cost bars ──────────────────────────────────────────────────────
const TOK_SEGS = [
  { key: 'input', color: 'var(--tok-input)',  label: 'input' },
  { key: 'cr',    color: 'var(--tok-cr)',     label: 'cache read' },
  { key: 'cw',    color: 'var(--tok-cw)',     label: 'cache write' },
  { key: 'out',   color: 'var(--tok-out)',    label: 'output' },
];

function DualBars({ usage, model, maxCost, costUsd, subagentCost = 0 }) {
  const M1 = 1_000_000;
  const tokPct = Math.min(100, (usage.total / M1) * 100);
  const cost = useMemo(() => computeCost(usage, model), [usage, model]);
  const displayCost = costUsd != null ? costUsd : cost.total;
  const totalCost = displayCost + subagentCost;
  const costPct = maxCost > 0 ? Math.min(100, (totalCost / maxCost) * 100) : 0;
  const tokSegs = TOK_SEGS.map(s => ({ ...s, val: usage[s.key] || 0 })).filter(s => s.val > 0);
  const costSegsRaw = [
    { key: 'input', val: cost.costInput, color: 'var(--tok-input)' },
    { key: 'cr',    val: cost.costCr,    color: 'var(--tok-cr)' },
    { key: 'cw',    val: cost.costCw,    color: 'var(--tok-cw)' },
    { key: 'out',   val: cost.costOut,   color: 'var(--tok-out)' },
    ...(subagentCost > 0 ? [{ key: 'sub', val: subagentCost, color: 'var(--tok-subagent)' }] : []),
  ].filter(s => s.val > 0);
  const tokTitle = tokSegs.map(s => `${s.label}: ${s.val.toLocaleString()}`).join(' | ') + ` | total: ${usage.total.toLocaleString()}`;
  const costTitle = `input: ${fmtCost(cost.costInput)} | cache read: ${fmtCost(cost.costCr)} | cache write: ${fmtCost(cost.costCw)} | output: ${fmtCost(cost.costOut)}`
    + (subagentCost > 0 ? ` | subagents: ${fmtCost(subagentCost)}` : '')
    + ` | total: ${fmtCost(totalCost)}`;
  return (
    <div className="bars">
      <div className="bar-row">
        <span className="bar-label" style={{ fontSize: 9 }}>tok</span>
        <div className="bar-track" title={tokTitle}>
          <div className="bar-fill" style={{ width: `${tokPct}%` }}>
            {tokSegs.map(s => <div key={s.key} className="bar-seg" style={{ flex: s.val, background: s.color }} />)}
          </div>
        </div>
        <span className="bar-value">{fmtTok(usage.total)}</span>
      </div>
      <div className="bar-row">
        <span className="bar-label" style={{ fontSize: 9 }}>cost</span>
        <div className="bar-track" title={costTitle}>
          <div className="bar-fill" style={{ width: `${costPct}%` }}>
            {costSegsRaw.map(s => <div key={s.key} className="bar-seg" style={{ flex: s.val, background: s.color }} />)}
          </div>
        </div>
        <span className="bar-value" style={{ color: '#7ee787' }}>{fmtCost(totalCost)}</span>
      </div>
    </div>
  );
}

// ── UserCard ───────────────────────────────────────────────────────────────
function UserCard({ item }) {
  const [expanded, setExpanded] = useState(false);
  const firstLine = item.text.split('\n')[0];
  const preview = firstLine.length > 120 ? firstLine.slice(0, 120) + '…' : firstLine;
  return (
    <div className="card card-user card-clickable" onClick={() => setExpanded(e => !e)}>
      <div className="user-header">
        <span style={{ color: 'var(--accent)' }}>👤</span>
        <span style={{ fontWeight: 600, fontSize: 11, color: 'var(--accent)' }}>User</span>
        {item.timestamp && <span style={{ color: 'var(--muted)', fontSize: 10 }}>{new Date(item.timestamp).toLocaleTimeString()}</span>}
        <span className="expand-chevron">{expanded ? '▲' : '▼'}</span>
      </div>
      {expanded
        ? <div className="user-text">{item.text}</div>
        : <div className="card-preview">{preview}</div>
      }
    </div>
  );
}

// ── ApiCard ────────────────────────────────────────────────────────────────
function ApiCard({ item, maxCost, sessionId, id, agentCache = {} }) {
  const [expanded, setExpanded] = useState(false);
  const usage = useMemo(() => getUsage(item), [item]);
  const shortModelName = item.model ? item.model.replace('claude-', '').replace(/-\d{8}$/, '') : '';
  const toolUses = item.tool_uses || [];
  const texts = item.texts || [];
  const hasDetails = texts.length > 0 || toolUses.length > 0;
  const subagentTotalCost = useMemo(
    () => toolUses.reduce((s, tu) => s + (tu.subagent_cost_usd || 0), 0),
    [toolUses]
  );
  return (
    <div id={id} className="card card-api" style={hasDetails ? { cursor: 'pointer' } : {}} onClick={hasDetails ? () => setExpanded(e => !e) : undefined}>
      <div className="api-header">
        <span className="api-num">API #{item.num}</span>
        <span className="api-cost">{fmtCost(item.cost_usd)}</span>
        {subagentTotalCost > 0 && (
          <span style={{ color: 'var(--tok-subagent)', fontSize: 11, fontWeight: 600 }}>
            +{fmtCost(subagentTotalCost)} 🤖
          </span>
        )}
        {shortModelName && <span className="api-model">{shortModelName}</span>}
        {item.timestamp && <span style={{ color: 'var(--muted)', fontSize: 10 }}>{new Date(item.timestamp).toLocaleTimeString()}</span>}
        {hasDetails && <span className="expand-chevron">{expanded ? '▲' : '▼'}</span>}
      </div>
      <DualBars usage={usage} model={item.model} maxCost={maxCost} costUsd={item.cost_usd} subagentCost={subagentTotalCost} />
      <div className="tok-stats">
        {[
          { label: 'in', val: usage.input, color: 'var(--tok-input)' },
          { label: 'cr', val: usage.cr,    color: 'var(--tok-cr)' },
          { label: 'cw', val: usage.cw,    color: 'var(--tok-cw)' },
          { label: 'out',val: usage.out,   color: 'var(--tok-out)' },
        ].filter(s => s.val > 0).map(s => (
          <span key={s.label} className="tok-stat">
            <span className="tok-dot" style={{ background: s.color }} />
            {s.label}:{fmtTok(s.val)}
          </span>
        ))}
        {subagentTotalCost > 0 && (
          <span className="tok-stat">
            <span className="tok-dot" style={{ background: 'var(--tok-subagent)' }} />
            🤖:{fmtCost(subagentTotalCost)}
          </span>
        )}
      </div>
      <div className="pills">
        {item.has_thinking && <span className="pill think">💭 thinking</span>}
        {texts.map((t, i) => (
          <span key={i} className="pill txt"
            style={{ maxWidth: 280, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
            title={t}>
            💬 {t.slice(0, 40)}{t.length > 40 ? '…' : ''}
          </span>
        ))}
        {toolUses.map(tu => {
          let label;
          if (tu.agent_id) {
            const model = shortModel(tu.input?.model || item.model);
            const desc = (tu.input?.description || tu.input?.prompt || '').replace(/\n/g, ' ');
            const descShort = desc.length > 45 ? desc.slice(0, 45) + '…' : desc;
            label = `Agent(${model} - ${descShort})`;
          } else {
            label = tu.summary || tu.name;
          }
          return (
            <span key={tu.id} className={`pill ${tu.agent_id ? 'agent' : 'tool'}`}>
              {tu.agent_id ? '🤖' : '🔧'} {label}
            </span>
          );
        })}
      </div>
      {expanded && (
        <div onClick={ev => ev.stopPropagation()} style={{ display: 'flex', flexDirection: 'column', gap: 8, marginTop: 8 }}>
          {texts.map((t, i) => (
            <div key={i} className="text-card">
              <div className="text-card-header">
                <span>💬</span>
                <span className="text-card-title">Message</span>
              </div>
              <div className="text-card-body">{t}</div>
            </div>
          ))}
          {toolUses.map((tu, i) =>
            tu.agent_id
              ? <SubagentCard key={i} item={tu} sessionId={sessionId} preloaded={agentCache[tu.agent_id]} />
              : <ToolResultCard key={i} item={tu} startExpanded={true} />
          )}
        </div>
      )}
    </div>
  );
}

// ── ToolResultCard ─────────────────────────────────────────────────────────
function ToolResultCard({ item, startExpanded = false }) {
  const [expanded, setExpanded] = useState(startExpanded);
  const content = item.result || '';
  const preview = content.replace(/\n/g, ' ').slice(0, 100);
  return (
    <div className="tool-card">
      <div className="tool-card-header" onClick={() => setExpanded(e => !e)}>
        <span>🔧</span>
        <span className="tool-card-title">{item.name}</span>
        {!expanded && preview && (
          <span style={{ color: 'var(--muted)', fontSize: 10, flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {preview}{content.length > 100 ? '…' : ''}
          </span>
        )}
        <span className="expand-chevron">{expanded ? '▲' : '▼'}</span>
      </div>
      {expanded && (
        <div className="tool-card-body">
          <TruncText text={content || '(empty)'} limit={2000} className="tool-content" />
        </div>
      )}
    </div>
  );
}

// ── SubagentCard ───────────────────────────────────────────────────────────
function SubagentCard({ item, sessionId, preloaded }) {
  const [timeline, setTimeline] = useState(preloaded || null);
  const [loading, setLoading] = useState(!preloaded);
  const [err, setErr] = useState(null);

  const desc = item.input?.description || item.input?.prompt || 'Agent';
  const agentType = item.input?.subagent_type || '';
  const model = shortModel(item.input?.model || '');

  // Use preloaded data if it arrives after mount (race between mount and preload)
  useEffect(() => {
    if (preloaded) { setTimeline(preloaded); setLoading(false); }
  }, [preloaded]);

  useEffect(() => {
    if (preloaded) return; // already handled above
    setLoading(true); setErr(null);
    fetch(`/api/subagent?session=${encodeURIComponent(sessionId)}&agent=${encodeURIComponent(item.agent_id)}`)
      .then(r => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then(data => { setTimeline(data.entries || []); setLoading(false); })
      .catch(e => { setErr(e.message); setLoading(false); });
  }, [sessionId, item.agent_id]);

  const apiItems = useMemo(() => (timeline || []).filter(i => i.kind === 'assistant'), [timeline]);
  const maxCost  = useMemo(() => Math.max(1e-9, apiItems.reduce((s, i) => s + (i.cost_usd || 0), 0)), [apiItems]);
  const totals   = useMemo(() => {
    const zero = { input: 0, cr: 0, cw: 0, out: 0, total: 0 };
    return apiItems.reduce((a, i) => {
      const u = getUsage(i);
      return { input: a.input+u.input, cr: a.cr+u.cr, cw: a.cw+u.cw, out: a.out+u.out, total: a.total+u.total };
    }, zero);
  }, [apiItems]);
  const totalCost = useMemo(() => apiItems.reduce((s, i) => s + (i.cost_usd || 0), 0), [apiItems]);

  return (
    <div className="agent-card">
      <div className="agent-card-header">
        <span>🤖</span>
        {model && <span style={{ color: 'var(--muted)', fontSize: 10, fontWeight: 500, flexShrink: 0 }}>{model}</span>}
        <span className="agent-card-title">{desc}</span>
        {agentType && <span className="subagent-badge">{agentType}</span>}
        {loading && <span className="spinner" />}
        <span style={{ display: 'flex', gap: 8, alignItems: 'center', fontSize: 10, color: 'var(--muted)', flexShrink: 0 }}>
          {totals.total > 0 && <span>{fmtTok(totals.total)} tok</span>}
          {totalCost > 0 && <span style={{ color: '#7ee787' }}>{fmtCost(totalCost)}</span>}
        </span>
      </div>
      <div className="agent-card-body">
        {err && <div className="err">Error: {err}</div>}
        {timeline && timeline.map((it, idx) => renderItem(it, idx, maxCost, sessionId))}
      </div>
    </div>
  );
}

// ── CostChart ──────────────────────────────────────────────────────────────
// `rangeNums` (optional): [start,end] API nums currently active as a filter.
// `onRangeChange(start,end | null)`: commit a drag-selected range.
function CostChart({ apiItems, rangeNums = null, onRangeChange = null }) {
  if (apiItems.length < 2) return null;
  if (!AreaChart) return null;

  const points = useMemo(() => {
    let cum = 0;
    return apiItems.map((item, i) => {
      const delta = item.cost_usd || 0;
      cum += delta;
      const toolUses = item.tool_uses || [];
      const activity = toolUses.length > 0
        ? toolUses.map(tu => tu.name).join('+')
        : item.has_thinking ? 'Thinking' : 'Text';
      return { cum, delta, activity, num: i + 1, timelineIdx: item.timelineIdx,
               entryId: item.entry_id ?? null,
               hasTool: toolUses.length > 0, hasThinking: item.has_thinking };
    });
  }, [apiItems]);

  const maxCum = points[points.length - 1].cum;

  const scrollToPoint = useCallback((payload) => {
    if (!payload) return;
    const el =
      (payload.entryId != null && document.getElementById(`entry-${payload.entryId}`)) ||
      document.getElementById(`api-item-${payload.timelineIdx}`);
    if (!el) return;
    el.scrollIntoView({ behavior: 'smooth', block: 'center' });
    el.classList.remove('highlight-flash');
    void el.offsetWidth; // force reflow so re-adding class restarts animation
    el.classList.add('highlight-flash');
    setTimeout(() => el.classList.remove('highlight-flash'), 15000);
  }, []);

  const [dragStart, setDragStart] = useState(null);
  const [dragEnd, setDragEnd] = useState(null);
  const dragRef = useRef({ start: null, end: null });
  dragRef.current = { start: dragStart, end: dragEnd };

  const commitDrag = useCallback(() => {
    const { start, end } = dragRef.current;
    setDragStart(null);
    setDragEnd(null);
    if (start == null || end == null) return;
    if (start === end) return; // click — let dot handler scroll instead
    if (!onRangeChange) return;
    const a = Math.min(start, end);
    const b = Math.max(start, end);
    if (a === 1 && b === points.length) onRangeChange(null, null);
    else onRangeChange(a, b);
  }, [onRangeChange, points.length]);

  useEffect(() => {
    if (dragStart == null) return;
    const onUp = () => commitDrag();
    document.addEventListener('mouseup', onUp);
    return () => document.removeEventListener('mouseup', onUp);
  }, [dragStart, commitDrag]);

  const onChartMouseDown = useCallback((e) => {
    if (!onRangeChange || !e || e.activeLabel == null) return;
    const n = Number(e.activeLabel);
    setDragStart(n);
    setDragEnd(n);
  }, [onRangeChange]);

  const onChartMouseMove = useCallback((e) => {
    if (dragStart == null || !e || e.activeLabel == null) return;
    setDragEnd(Number(e.activeLabel));
  }, [dragStart]);

  const inRange = useCallback((num) => {
    if (!rangeNums) return true;
    return num >= rangeNums[0] && num <= rangeNums[1];
  }, [rangeNums]);

  const CustomDot = useCallback((props) => {
    const { cx, cy, payload } = props;
    if (cx == null || cy == null) return null;
    const color = payload.hasTool ? '#7ee787' : payload.hasThinking ? '#d2a8ff' : '#58a6ff';
    const dim = !inRange(payload.num);
    return (
      <circle
        cx={cx} cy={cy} r={4}
        fill={color}
        opacity={dim ? 0.3 : 1}
        style={{ cursor: 'pointer' }}
        onClick={() => scrollToPoint(payload)}
      />
    );
  }, [scrollToPoint, inRange]);

  const CustomTooltip = ({ active, payload }) => {
    if (!active || !payload?.length) return null;
    const p = payload[0].payload;
    return (
      <div style={{ background: 'var(--surface2)', border: '1px solid var(--border)', borderRadius: 4, padding: '4px 8px', fontSize: 11, lineHeight: 1.5 }}>
        <div>API #{p.num}: <strong>+{fmtCost(p.delta)}</strong> → <strong>{fmtCost(p.cum)}</strong></div>
        <div style={{ color: 'var(--muted)' }}>{p.activity}</div>
        <div style={{ color: 'var(--muted)', fontSize: 10 }}>
          {onRangeChange ? 'click: scroll · drag: select range' : 'click to scroll'}
        </div>
      </div>
    );
  };

  return (
    <div style={{ padding: '5px 16px 0', background: 'var(--surface2)', borderBottom: '1px solid var(--border)', flexShrink: 0, userSelect: dragStart != null ? 'none' : undefined }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, fontSize: 9, color: 'var(--muted)', marginBottom: 2 }}>
        <span>Cumulative cost · <strong style={{ color: '#7ee787' }}>{fmtCost(maxCum)}</strong></span>
        <span style={{ display: 'flex', gap: 8 }}>
          <span><span style={{ color: '#7ee787' }}>●</span> tool</span>
          <span><span style={{ color: '#d2a8ff' }}>●</span> think</span>
          <span><span style={{ color: '#58a6ff' }}>●</span> text</span>
        </span>
        {rangeNums && (
          <span style={{ marginLeft: 'auto', color: 'var(--accent)' }}>
            range: API #{rangeNums[0]}–#{rangeNums[1]}
            {onRangeChange && (
              <button
                onClick={() => onRangeChange(null, null)}
                style={{ marginLeft: 6, background: 'none', border: 'none', color: 'var(--muted)', cursor: 'pointer', fontSize: 10, padding: 0, textDecoration: 'underline' }}
              >
                clear
              </button>
            )}
          </span>
        )}
      </div>
      <ResponsiveContainer width="100%" height={55}>
        <AreaChart
          data={points}
          margin={{ top: 4, right: 4, left: 4, bottom: 4 }}
          onMouseDown={onChartMouseDown}
          onMouseMove={onChartMouseMove}
          onMouseUp={commitDrag}
          style={{ cursor: onRangeChange ? 'crosshair' : undefined }}
        >
          <defs>
            <linearGradient id="costGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="var(--accent)" stopOpacity={0.15} />
              <stop offset="95%" stopColor="var(--accent)" stopOpacity={0} />
            </linearGradient>
          </defs>
          <XAxis dataKey="num" hide type="number" domain={[1, points.length]} />
          <YAxis hide domain={[0, maxCum * 1.05]} />
          <Tooltip content={<CustomTooltip />} />
          <Area
            type="monotone"
            dataKey="cum"
            stroke="var(--accent)"
            strokeWidth={1.5}
            fill="url(#costGradient)"
            dot={<CustomDot />}
            activeDot={{ r: 5, style: { cursor: 'pointer' }, onClick: (_, p) => scrollToPoint(p.payload) }}
            isAnimationActive={false}
          />
          {dragStart != null && dragEnd != null && dragStart !== dragEnd && (
            <ReferenceArea
              x1={Math.min(dragStart, dragEnd)}
              x2={Math.max(dragStart, dragEnd)}
              strokeOpacity={0}
              fill="#58a6ff"
              fillOpacity={0.22}
            />
          )}
          {dragStart == null && rangeNums && (
            <ReferenceArea
              x1={rangeNums[0]}
              x2={rangeNums[1]}
              strokeOpacity={0}
              fill="#58a6ff"
              fillOpacity={0.1}
            />
          )}
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}

// ── ActivityBreakdown ──────────────────────────────────────────────────────
function ActivityBreakdown({ timeline }) {
  const apiItems = useMemo(() => timeline.filter(i => i.kind === 'assistant'), [timeline]);

  const breakdown = useMemo(() => {
    const map = {};
    for (const item of apiItems) {
      const cost = item.cost_usd || 0;
      const toolUses = item.tool_uses || [];
      if (toolUses.length > 0) {
        const uniqueTools = [...new Set(toolUses.map(tu => tu.name))];
        const share = cost / uniqueTools.length;
        for (const toolName of uniqueTools) {
          if (!map[toolName]) map[toolName] = { cost: 0, calls: 0 };
          map[toolName].cost += share;
          map[toolName].calls += toolUses.filter(tu => tu.name === toolName).length;
        }
      } else if (item.has_thinking) {
        if (!map['Thinking']) map['Thinking'] = { cost: 0, calls: 0 };
        map['Thinking'].cost += cost;
        map['Thinking'].calls += 1;
      } else {
        if (!map['Text']) map['Text'] = { cost: 0, calls: 0 };
        map['Text'].cost += cost;
        map['Text'].calls += 1;
      }
    }
    const total = Object.values(map).reduce((s, v) => s + v.cost, 0);
    return Object.entries(map)
      .map(([name, { cost, calls }]) => ({ name, cost, calls, pct: total > 0 ? (cost / total) * 100 : 0 }))
      .sort((a, b) => b.cost - a.cost);
  }, [apiItems]);

  if (breakdown.length === 0) return null;

  return (
    <div style={{
      padding: '5px 16px', background: 'var(--surface2)', borderBottom: '1px solid var(--border)',
      display: 'flex', flexWrap: 'wrap', gap: '3px 14px', alignItems: 'center', flexShrink: 0,
    }}>
      <span style={{ color: 'var(--muted)', fontSize: 10, whiteSpace: 'nowrap' }}>By activity:</span>
      {breakdown.map(row => (
        <span key={row.name} style={{ display: 'inline-flex', alignItems: 'baseline', gap: 3, fontSize: 10 }}>
          <span style={{ color: 'var(--text)', fontWeight: 500 }}>{row.name}</span>
          <span style={{ color: '#7ee787', fontWeight: 600 }}>{fmtCost(row.cost)}</span>
          <span style={{ color: 'var(--muted)' }}>({row.pct.toFixed(0)}%)</span>
          <span style={{ color: 'var(--muted)' }}>×{row.calls}</span>
        </span>
      ))}
    </div>
  );
}

// ── Session totals ─────────────────────────────────────────────────────────
function SessionTotals({ timeline }) {
  const apiItems = useMemo(() => timeline.filter(i => i.kind === 'assistant'), [timeline]);
  const totals   = useMemo(() => {
    const zero = { input: 0, cr: 0, cw: 0, out: 0, total: 0 };
    return apiItems.reduce((a, i) => {
      const u = getUsage(i);
      return { input: a.input+u.input, cr: a.cr+u.cr, cw: a.cw+u.cw, out: a.out+u.out, total: a.total+u.total };
    }, zero);
  }, [apiItems]);
  const cost = useMemo(() => apiItems.reduce((s, i) => s + (i.cost_usd || 0), 0), [apiItems]);

  return (
    <div className="totals">
      <strong style={{ color: '#7ee787', fontSize: 12 }}>{fmtCost(cost)}</strong>
      <span className="sep">·</span>
      <span style={{ color: 'var(--muted)' }}>{apiItems.length} API calls</span>
      <span className="sep">·</span>
      {[
        { key: 'input', label: 'in',  color: 'var(--tok-input)' },
        { key: 'cr',    label: 'cr',  color: 'var(--tok-cr)' },
        { key: 'cw',    label: 'cw',  color: 'var(--tok-cw)' },
        { key: 'out',   label: 'out', color: 'var(--tok-out)' },
      ].filter(b => totals[b.key] > 0).map(b => (
        <span key={b.key} className="totals-badge">
          <span className="t-dot" style={{ background: b.color }} />
          <span style={{ color: 'var(--muted)' }}>{b.label}</span>
          <strong style={{ color: 'var(--text)' }}>{fmtTok(totals[b.key])}</strong>
        </span>
      ))}
      <span style={{ color: 'var(--muted)', marginLeft: 'auto', fontSize: 10 }}>
        {fmtTok(totals.total)} total tokens
      </span>
    </div>
  );
}

// ── CompactCard ────────────────────────────────────────────────────────────
function CompactCard({ item }) {
  const [showTools, setShowTools] = React.useState(false);
  const isMicro = item.subtype === 'microcompact_boundary';
  const label   = isMicro ? 'Microcompact' : 'Compact';
  const preTok  = item.pre_tokens  != null ? fmtTok(item.pre_tokens)  : null;
  const postTok = item.post_tokens != null ? fmtTok(item.post_tokens) : null;
  const pct     = item.reduction_pct != null ? `${item.reduction_pct}% reduction` : null;
  const dur     = item.duration_ms  != null ? `${(item.duration_ms / 1000).toFixed(1)}s` : null;
  const tools   = item.pre_discovered_tools;
  return (
    <div className="card card-compact">
      <div className="compact-header">
        <span style={{ color: '#f59e0b' }}>⚡</span>
        <span style={{ fontWeight: 600, fontSize: 11, color: '#f59e0b' }}>{label}</span>
        {item.trigger && <span style={{ color: 'var(--muted)', fontSize: 11 }}>{item.trigger}</span>}
        {item.timestamp && <span style={{ color: 'var(--muted)', fontSize: 10 }}>{new Date(item.timestamp).toLocaleTimeString()}</span>}
      </div>
      <div className="summary-row">
        {preTok  && <span>before: <strong>{preTok}</strong></span>}
        {postTok && <span>after: <strong>{postTok}</strong></span>}
        {pct     && <span style={{ color: '#f59e0b' }}>{pct}</span>}
        {dur     && <span style={{ color: 'var(--muted)' }}>{dur}</span>}
        {tools && tools.length > 0 && (
          <span
            style={{ color: 'var(--accent)', cursor: 'pointer', textDecoration: 'underline', fontSize: 11 }}
            onClick={() => setShowTools(v => !v)}
          >
            {tools.length} tools {showTools ? '▲' : '▼'}
          </span>
        )}
      </div>
      {showTools && tools && (
        <div style={{ marginTop: 6, display: 'flex', flexWrap: 'wrap', gap: 4 }}>
          {tools.map(t => (
            <span key={t} style={{ background: 'var(--surface2)', border: '1px solid var(--border)', borderRadius: 4, padding: '1px 6px', fontSize: 10, color: 'var(--muted)', fontFamily: 'monospace' }}>{t}</span>
          ))}
        </div>
      )}
    </div>
  );
}

// ── renderItem ─────────────────────────────────────────────────────────────
function renderItem(item, idx, maxCost, sessionId, agentCache = {}) {
  if (item.kind === 'user')      return <UserCard    key={idx} item={item} />;
  if (item.kind === 'assistant') return <ApiCard     key={idx} id={item.entry_id != null ? `entry-${item.entry_id}` : `api-item-${idx}`} item={item} maxCost={maxCost} sessionId={sessionId} agentCache={agentCache} />;
  if (item.kind === 'compact')   return <CompactCard key={idx} item={item} />;
  return null;
}

// ── URL param sync ─────────────────────────────────────────────────────────
function getUrlParams() {
  const p = new URLSearchParams(window.location.search);
  return {
    project: p.get('project') || '',
    session: p.get('session') || '',
    sort: p.get('sort') || 'date',
    tab: p.get('tab') || 'dashboard',
    range: p.get('range') || '30d',
    entry: p.get('entry') || '',
  };
}

// ── Time range helpers ─────────────────────────────────────────────────────
function timeParams(timeRange) {
  if (timeRange === 'all') return '';
  const days = timeRange === '7d' ? 7 : timeRange === '30d' ? 30 : 90;
  const from = new Date(Date.now() - days * 86400000).toISOString();
  const to = new Date().toISOString();
  return `from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`;
}

function buildUrl(path, timeRange, extraParams = {}) {
  const parts = [];
  const tp = timeParams(timeRange);
  if (tp) parts.push(tp);
  for (const [k, v] of Object.entries(extraParams)) {
    if (v != null && v !== '') parts.push(`${k}=${encodeURIComponent(v)}`);
  }
  return parts.length ? `${path}?${parts.join('&')}` : path;
}

// ── usePanel hook ──────────────────────────────────────────────────────────
function usePanel(url) {
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState(null);
  useEffect(() => {
    if (!url) { setData(null); return; }
    setLoading(true); setData(null); setErr(null);
    fetch(url)
      .then(r => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
      .then(d => { setData(d); setLoading(false); })
      .catch(e => { setErr(e.message); setLoading(false); });
  }, [url]);
  return { data, loading, err };
}

// ── Panel wrapper ──────────────────────────────────────────────────────────
function Panel({ title, meta, loading, err, children }) {
  return (
    <div className="panel">
      <div className="panel-title">
        {title}
        {meta && <span className="panel-meta">{meta}</span>}
      </div>
      {loading && <div className="panel-loading">Loading…</div>}
      {err && <div className="panel-error">Error: {err}</div>}
      {!loading && !err && children}
    </div>
  );
}

// ── SummaryBar ─────────────────────────────────────────────────────────────
function SummaryBar({ data }) {
  if (!data) return null;
  return (
    <div className="stat-cards">
      <div className="stat-card">
        <div className="stat-label">Total Spend</div>
        <div className="stat-value" style={{ color: '#7ee787' }}>{fmtCost(data.cost_usd || 0)}</div>
      </div>
      <div className="stat-card">
        <div className="stat-label">Sessions</div>
        <div className="stat-value">{fmtNum((data.session_count || 0) + (data.subagent_count || 0))}</div>
        <div className="stat-sub">{fmtNum(data.session_count || 0)} main + {fmtNum(data.subagent_count || 0)} subagents</div>
      </div>
      <div className="stat-card">
        <div className="stat-label">API Calls</div>
        <div className="stat-value">{fmtNum(data.api_call_count || 0)}</div>
      </div>
      <div className="stat-card">
        <div className="stat-label">Avg Cost / Session</div>
        <div className="stat-value" style={{ color: '#7ee787' }}>{fmtCost(data.avg_cost_per_session || 0)}</div>
      </div>
    </div>
  );
}

// ── DailySpendChart ────────────────────────────────────────────────────────
function DailySpendChart({ data }) {
  if (!BarChart) {
    return <div className="panel-error">Recharts not loaded</div>;
  }
  if (!data || data.length === 0) {
    return <div className="panel-loading">No data</div>;
  }
  const chartData = data.map(d => ({
    date: d.date,
    label: (() => {
      try { return new Date(d.date).toLocaleDateString(undefined, { month: 'short', day: 'numeric' }); }
      catch { return d.date; }
    })(),
    Opus: d.cost_opus || 0,
    Sonnet: d.cost_sonnet || 0,
    Haiku: d.cost_haiku || 0,
  }));
  return (
    <div style={{ width: '100%', height: 200 }}>
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={chartData} margin={{ top: 6, right: 10, left: 0, bottom: 4 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="#21262d" />
          <XAxis dataKey="label" stroke="#8b949e" tick={{ fontSize: 10 }} />
          <YAxis stroke="#8b949e" tick={{ fontSize: 10 }} tickFormatter={v => `$${v}`} />
          <Tooltip
            contentStyle={{ background: '#1c2128', border: '1px solid #30363d', fontSize: 11 }}
            formatter={(v, n) => [fmtCost(v), n]}
          />
          <Legend wrapperStyle={{ fontSize: 11 }} />
          <Bar dataKey="Opus" stackId="a" fill="#f59e0b" />
          <Bar dataKey="Sonnet" stackId="a" fill="#3b82f6" />
          <Bar dataKey="Haiku" stackId="a" fill="#22c55e" />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

// ── TwoRegimeChart ─────────────────────────────────────────────────────────
function TwoRegimeChart({ data }) {
  if (!data || data.length === 0) return <div className="panel-loading">No data</div>;
  const { ComposedChart, Bar, Line, XAxis, YAxis, Tooltip, Legend, ResponsiveContainer } = window.Recharts || {};
  if (!ComposedChart) return <div className="panel-error">Recharts not loaded</div>;
  const chartData = data.map(d => ({
    week: (d.week || '').slice(5, 10),
    session_count: d.session_count || 0,
    median_cost: d.median_cost || 0,
    p90_cost: d.p90_cost || 0,
  }));
  return (
    <div style={{ width: '100%', height: 220 }}>
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart data={chartData} margin={{ top: 6, right: 50, left: 0, bottom: 4 }}>
          <XAxis dataKey="week" stroke="#8b949e" tick={{ fontSize: 10 }} />
          <YAxis yAxisId="left" stroke="#8b949e" tick={{ fontSize: 10 }} label={{ value: 'sessions', angle: -90, position: 'insideLeft', style: { fontSize: 10, fill: '#8b949e' } }} />
          <YAxis yAxisId="right" orientation="right" stroke="#8b949e" tick={{ fontSize: 10 }} tickFormatter={v => `$${v}`} label={{ value: '$/session', angle: 90, position: 'insideRight', style: { fontSize: 10, fill: '#8b949e' } }} />
          <Tooltip
            contentStyle={{ background: '#1c2128', border: '1px solid #30363d', fontSize: 11 }}
            formatter={(v, n) => n === 'sessions' ? [v, n] : [fmtCost(v), n]}
          />
          <Legend wrapperStyle={{ fontSize: 11 }} />
          <Bar yAxisId="left" dataKey="session_count" name="sessions" fill="#3b82f6" opacity={0.7} />
          <Line yAxisId="right" type="monotone" dataKey="median_cost" name="median $/session" stroke="#22c55e" strokeWidth={2} dot={{ r: 3 }} />
          <Line yAxisId="right" type="monotone" dataKey="p90_cost" name="p90 $/session" stroke="#f59e0b" strokeWidth={2} dot={{ r: 3 }} strokeDasharray="4 2" />
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  );
}

// ── FirstTurnCcChart ────────────────────────────────────────────────────────
function FirstTurnCcChart({ data }) {
  if (!data || data.length === 0) return <div className="panel-loading">No data</div>;
  const { BarChart, Bar, XAxis, YAxis, Tooltip, Legend, ResponsiveContainer } = window.Recharts || {};
  if (!BarChart) return <div className="panel-error">Recharts not loaded</div>;
  const chartData = data.map(d => ({
    bucket: d.bucket,
    main: d.main_sessions || 0,
    subagent: d.subagent_sessions || 0,
  }));
  return (
    <div style={{ width: '100%', height: 200 }}>
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={chartData} margin={{ top: 6, right: 10, left: 0, bottom: 4 }}>
          <XAxis dataKey="bucket" stroke="#8b949e" tick={{ fontSize: 10 }} />
          <YAxis stroke="#8b949e" tick={{ fontSize: 10 }} />
          <Tooltip
            contentStyle={{ background: '#1c2128', border: '1px solid #30363d', fontSize: 11 }}
            formatter={(v, n) => [v, n]}
          />
          <Legend wrapperStyle={{ fontSize: 11 }} />
          <Bar dataKey="main" name="main sessions" fill="#3b82f6" />
          <Bar dataKey="subagent" name="subagent sessions" fill="#a855f7" />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

// ── ModelBreakdownTable ────────────────────────────────────────────────────
function ModelBreakdownTable({ data }) {
  if (!data || data.length === 0) return <div className="panel-loading">No data</div>;
  return (
    <table className="data-table">
      <thead>
        <tr>
          <th>Model</th>
          <th className="num">Sessions</th>
          <th className="num">API Calls</th>
          <th className="num">Total Cost</th>
          <th className="num">% Spend</th>
          <th className="num">Avg $/Turn</th>
        </tr>
      </thead>
      <tbody>
        {data.map((r, i) => (
          <tr key={i}>
            <td>{shortModel(r.model)}</td>
            <td className="num">{fmtNum(r.sessions)}</td>
            <td className="num">{fmtNum(r.api_calls)}</td>
            <td className="cost">{fmtCost(r.cost_usd || 0)}</td>
            <td className="num">{fmtPct(r.pct_spend || 0)}</td>
            <td className="cost">{fmtCost(r.avg_cost_per_turn || 0)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

// ── CacheHealthPanel ───────────────────────────────────────────────────────
function CacheHealthPanel({ data }) {
  if (!data) return <div className="panel-loading">No data</div>;
  const thrash = data.thrash_turns || [];
  return (
    <>
      <div className="metric-row">
        <div className="metric">
          <div className="metric-label">Hit Rate</div>
          <div className="metric-value">{fmtPct(data.hit_rate || 0)}</div>
        </div>
        <div className="metric">
          <div className="metric-label">Create Rate</div>
          <div className="metric-value">{fmtPct(data.create_rate || 0)}</div>
        </div>
        <div className="metric">
          <div className="metric-label">Total Tokens</div>
          <div className="metric-value">{fmtTok(data.total_tokens || 0)}</div>
          <div className="metric-sub">
            CR: {fmtTok(data.cache_read_tokens || 0)} · CW: {fmtTok(data.cache_create_tokens || 0)}
          </div>
        </div>
      </div>
      {thrash.length > 0 && (
        <>
          <div style={{ fontSize: 11, color: 'var(--muted)', marginTop: 8, marginBottom: 6 }}>Top Thrash Turns</div>
          <table className="data-table">
            <thead>
              <tr>
                <th>Session</th>
                <th>Project</th>
                <th className="num">Cost</th>
                <th className="num">CC Tokens</th>
                <th className="num">Output Tokens</th>
              </tr>
            </thead>
            <tbody>
              {thrash.map((t, i) => (
                <tr key={i}>
                  <td>{(t.session_id || '').slice(0, 8)}</td>
                  <td title={t.project}>{shortPath(t.project, 2)}</td>
                  <td className="cost">{fmtCost(t.cost_usd || 0)}</td>
                  <td className="num">{fmtTok(t.cc_tokens || 0)}</td>
                  <td className="num">{fmtTok(t.output_tokens || 0)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </>
  );
}

// ── AgentModelPanel ────────────────────────────────────────────────────────
function AgentModelPanel({ data }) {
  if (!data) return <div className="panel-loading">No data</div>;
  const explicit = data.explicit_calls || 0;
  const inherited = data.inherited_calls || 0;
  const total = explicit + inherited;
  const inheritedPct = total > 0 ? inherited / total : 0;
  const subtypes = data.subtypes || [];
  return (
    <>
      <div className="metric-row">
        <div className="metric">
          <div className="metric-label">Explicit Calls</div>
          <div className="metric-value">{fmtNum(explicit)}</div>
        </div>
        <div className="metric">
          <div className="metric-label">Inherited Calls</div>
          <div className="metric-value">{fmtNum(inherited)}</div>
          <div className="metric-sub">{fmtPct(inheritedPct)} of total</div>
        </div>
        <div className="metric">
          <div className="metric-label">Inherited Cost</div>
          <div className="metric-value" style={{ color: '#7ee787' }}>{fmtCost(data.inherited_cost_usd || 0)}</div>
        </div>
      </div>
      {subtypes.length > 0 && (
        <>
          <div style={{ fontSize: 11, color: 'var(--muted)', marginTop: 8, marginBottom: 6 }}>Subtypes</div>
          <table className="data-table">
            <thead>
              <tr>
                <th>Subtype</th>
                <th className="num">Count</th>
              </tr>
            </thead>
            <tbody>
              {subtypes.map((s, i) => (
                <tr key={i}>
                  <td>{s.subtype || '(unknown)'}</td>
                  <td className="num">{fmtNum(s.count)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
      {(data.spawn_model_breakdown || []).length > 0 && (
        <>
          <div style={{ fontSize: 11, color: 'var(--muted)', marginTop: 12, marginBottom: 6 }}>By Subtype</div>
          <table className="data-table">
            <thead>
              <tr>
                <th>Subtype</th>
                <th className="num">Spawns</th>
                <th className="num">Explicit model</th>
                <th className="num">Inherited model</th>
              </tr>
            </thead>
            <tbody>
              {(data.spawn_model_breakdown || []).map((s, i) => (
                <tr key={i}>
                  <td>{s.subtype || '(unknown)'}</td>
                  <td className="num">{fmtNum(s.spawns)}</td>
                  <td className="num">{fmtNum(s.explicit)}</td>
                  <td className="num">{fmtNum(s.inherited)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </>
  );
}

// ── TopSessionsTable ───────────────────────────────────────────────────────
function TopSessionsTable({ data, projects, navigateToSession }) {
  if (!data || data.length === 0) return <div className="panel-loading">No data</div>;
  const projMap = useMemo(() => {
    const m = {};
    (projects || []).forEach(p => { m[p.key] = p.display; });
    return m;
  }, [projects]);
  return (
    <table className="data-table">
      <thead>
        <tr>
          <th>Session</th>
          <th>Project</th>
          <th>Started</th>
          <th className="num">Cost</th>
          <th className="num">Turns</th>
          <th className="num">Errors</th>
          <th className="num">Subagents</th>
        </tr>
      </thead>
      <tbody>
        {data.map((s, i) => (
          <tr key={i} style={{ cursor: 'pointer' }} onClick={() => navigateToSession(s.project, s.session_id)}>
            <td className="link">{(s.session_id || '').slice(0, 8)}</td>
            <td title={s.project}>{projMap[s.project] || shortPath(s.project, 2)}</td>
            <td>{fmtDate(s.started_at)}</td>
            <td className="cost">{fmtCost(s.cost_usd || 0)}</td>
            <td className="num">{fmtNum(s.turn_count || 0)}</td>
            <td className="num" style={{ color: (s.error_count || 0) > 0 ? '#f85149' : undefined }}>{fmtNum(s.error_count || 0)}</td>
            <td className="num">{fmtNum(s.subagent_count || 0)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

// ── SessionDistributionTable ───────────────────────────────────────────────
function SessionDistributionTable({ data }) {
  if (!data || data.length === 0) return <div className="panel-loading">No data</div>;
  return (
    <table className="data-table">
      <thead>
        <tr>
          <th>Bucket (turns)</th>
          <th className="num">Sessions</th>
          <th className="num">Total $</th>
          <th className="num">Avg $</th>
          <th className="num">Max $</th>
        </tr>
      </thead>
      <tbody>
        {data.map((r, i) => (
          <tr key={i}>
            <td>{r.bucket}</td>
            <td className="num">{fmtNum(r.session_count || 0)}</td>
            <td className="cost">{fmtCost(r.total_cost || 0)}</td>
            <td className="cost">{fmtCost(r.avg_cost || 0)}</td>
            <td className="cost">{fmtCost(r.max_cost || 0)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

// ── FileHotspotsTable ──────────────────────────────────────────────────────
function FileHotspotsTable({ data }) {
  if (!data || data.length === 0) return <div className="panel-loading">No data</div>;
  return (
    <table className="data-table">
      <thead>
        <tr>
          <th>File</th>
          <th className="num">Distinct Sessions</th>
          <th className="num">Total Reads</th>
        </tr>
      </thead>
      <tbody>
        {data.map((r, i) => (
          <tr key={i}>
            <td title={r.file_path}>{shortPath(r.file_path, 3)}</td>
            <td className="num">{fmtNum(r.distinct_sessions || 0)}</td>
            <td className="num">{fmtNum(r.total_reads || 0)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

// ── ErrorSummaryPanel ──────────────────────────────────────────────────────
function ErrorSummaryPanel({ data }) {
  if (!data) return <div className="panel-loading">No data</div>;
  const types = data.types || [];
  const buckets = data.by_bucket || [];
  return (
    <>
      <div style={{ fontSize: 11, color: 'var(--muted)', marginBottom: 6 }}>Error Types</div>
      {types.length === 0
        ? <div className="panel-loading">No errors</div>
        : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Type</th>
                <th className="num">Count</th>
                <th className="num">Sessions Affected</th>
              </tr>
            </thead>
            <tbody>
              {types.map((r, i) => (
                <tr key={i}>
                  <td>{r.error_type}</td>
                  <td className="num">{fmtNum(r.count || 0)}</td>
                  <td className="num">{fmtNum(r.sessions_affected || 0)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )
      }
      <div style={{ fontSize: 11, color: 'var(--muted)', marginTop: 12, marginBottom: 6 }}>$/Turn by Error Bucket</div>
      {buckets.length === 0
        ? <div className="panel-loading">No data</div>
        : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Bucket</th>
                <th className="num">Sessions</th>
                <th className="num">Avg $/Turn</th>
                <th className="num">Errors/Turn</th>
              </tr>
            </thead>
            <tbody>
              {buckets.map((r, i) => (
                <tr key={i}>
                  <td>{r.bucket}</td>
                  <td className="num">{fmtNum(r.sessions || 0)}</td>
                  <td className="cost">{fmtCost(r.avg_cost_per_turn || 0)}</td>
                  <td className="num">{(r.errors_per_turn || 0).toFixed(3)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )
      }
    </>
  );
}

// ── BaselineBar ────────────────────────────────────────────────────────────
function BaselineBar({ url }) {
  const { data, loading, err } = usePanel(url);
  if (loading) return <div className="baseline-bar muted">Loading baseline…</div>;
  if (err || !data) return null;
  const { mean_usd, median_usd, selected_usd, vs_mean, vs_median } = data;
  const fmtW = v => v >= 1000 ? `$${(v/1000).toFixed(1)}k` : `$${v.toFixed(0)}`;
  return (
    <div className="baseline-bar">
      <span><strong>Claude Code usage per week:</strong> <span className="muted">Median</span> {fmtW(median_usd)} <span className="muted">· Mean</span> {fmtW(mean_usd)}</span>
      <span className="dot">·</span>
      <span><strong>Selected:</strong> {fmtW(selected_usd)} <span className="muted">= {vs_median.toFixed(1)}× median, {vs_mean.toFixed(1)}× mean</span></span>
    </div>
  );
}

// ── TokenStreamsPanel ──────────────────────────────────────────────────────
function TokenStreamsPanel({ data }) {
  if (!BarChart) return <div className="panel-error">Recharts not loaded</div>;
  if (!data) return <div className="panel-loading">No data</div>;

  const { streams, reconciliation_delta } = data;

  const chartData = [
    {
      name: 'Main-chain',
      input:      (streams.main?.input      || 0),
      cc5m:       (streams.main?.cc5m       || 0),
      cc1h:       (streams.main?.cc1h       || 0),
      cache_read: (streams.main?.cache_read || 0),
      output:     (streams.main?.output     || 0),
    },
    {
      name: 'Sidechain',
      input:      (streams.sidechain?.input      || 0),
      cc5m:       (streams.sidechain?.cc5m       || 0),
      cc1h:       (streams.sidechain?.cc1h       || 0),
      cache_read: (streams.sidechain?.cache_read || 0),
      output:     (streams.sidechain?.output     || 0),
    },
  ];

  const fmtDollar = v => `$${v.toFixed(4)}`;
  const deltaAbs = Math.abs(reconciliation_delta || 0);
  const total = (streams.main?.total || 0) + (streams.sidechain?.total || 0);
  const deltaWarning = total > 0 && deltaAbs / total > 0.01;

  return (
    <div>
      <div style={{ width: '100%', height: 140 }}>
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={chartData} layout="vertical" margin={{ left: 80, right: 20, top: 4, bottom: 4 }}>
            <XAxis type="number" tickFormatter={v => `$${v.toFixed(2)}`} stroke="#8b949e" tick={{ fontSize: 10 }} />
            <YAxis type="category" dataKey="name" width={80} stroke="#8b949e" tick={{ fontSize: 10 }} />
            <Tooltip
              contentStyle={{ background: '#1c2128', border: '1px solid #30363d', fontSize: 11 }}
              formatter={fmtDollar}
            />
            <Legend wrapperStyle={{ fontSize: 11 }} />
            <Bar dataKey="input"      stackId="a" fill="#6baed6" name="input" />
            <Bar dataKey="cc5m"       stackId="a" fill="#fbbf24" name="cc5m" />
            <Bar dataKey="cc1h"       stackId="a" fill="#f97316" name="cc1h" />
            <Bar dataKey="cache_read" stackId="a" fill="#4ade80" name="cache read" />
            <Bar dataKey="output"     stackId="a" fill="#a78bfa" name="output" />
          </BarChart>
        </ResponsiveContainer>
      </div>
      <div className="muted" style={{ fontSize: '0.8em', marginTop: 4, color: deltaWarning ? 'var(--err, #e53e3e)' : undefined }}>
        Reconciliation delta: {reconciliation_delta >= 0 ? '+' : ''}{(reconciliation_delta || 0).toFixed(4)} (pricing derived from latest model_pricing row)
      </div>
    </div>
  );
}

// ── ArtifactLeaderboard ────────────────────────────────────────────────────
function ArtifactLeaderboard({ data, kind, navigateToSession }) {
  if (!data || !data.rows || data.rows.length === 0)
    return <div className="muted">No data</div>;

  const { rows } = data;
  const fmtSize = n => n == null ? '—' : n >= 1000 ? `${(n/1000).toFixed(1)}k` : String(n);

  if (kind === 'write') return (
    <table className="data-table">
      <thead><tr><th>File</th><th>Size</th><th>Session</th><th>Time</th></tr></thead>
      <tbody>{rows.map((r, i) => (
        <tr key={i} style={{ cursor: 'pointer' }}
            onClick={() => navigateToSession(r.project, r.session_id)}>
          <td style={{ maxWidth: 300, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
              title={r.file_path}>{r.file_path || '—'}</td>
          <td className="num">{fmtSize(r.size_chars)} chars</td>
          <td style={{ fontFamily: 'monospace', fontSize: '0.85em' }}>{(r.session_id || '').slice(0,8)}</td>
          <td style={{ whiteSpace: 'nowrap', fontSize: '0.85em' }}>{(r.ts || '').slice(0,16)}</td>
        </tr>
      ))}</tbody>
    </table>
  );

  if (kind === 'agent') return (
    <table className="data-table">
      <thead><tr><th>Type</th><th>Description</th><th>Prompt size</th><th>Session</th><th>Time</th></tr></thead>
      <tbody>{rows.map((r, i) => (
        <tr key={i} style={{ cursor: 'pointer' }}
            onClick={() => navigateToSession(r.project, r.session_id)}>
          <td>{r.subagent_type || '—'}</td>
          <td style={{ maxWidth: 250, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
              title={r.description}>{r.description || '—'}</td>
          <td className="num">{fmtSize(r.size_chars)} chars</td>
          <td style={{ fontFamily: 'monospace', fontSize: '0.85em' }}>{(r.session_id || '').slice(0,8)}</td>
          <td style={{ whiteSpace: 'nowrap', fontSize: '0.85em' }}>{(r.ts || '').slice(0,16)}</td>
        </tr>
      ))}</tbody>
    </table>
  );

  // tool_result
  return (
    <table className="data-table">
      <thead><tr><th>Tool</th><th>Label</th><th>Size</th><th>Session</th><th>Time</th></tr></thead>
      <tbody>{rows.map((r, i) => {
        const label = r.label_file || r.label_cmd || r.label_url || r.label_pat || '—';
        return (
          <tr key={i} style={{ cursor: 'pointer' }}
              onClick={() => navigateToSession(r.project, r.session_id)}>
            <td>{r.tool_name || '—'}</td>
            <td style={{ maxWidth: 260, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
                title={label}>{label}</td>
            <td className="num">{fmtSize(r.size_chars)} chars</td>
            <td style={{ fontFamily: 'monospace', fontSize: '0.85em' }}>{(r.session_id || '').slice(0,8)}</td>
            <td style={{ whiteSpace: 'nowrap', fontSize: '0.85em' }}>{(r.ts || '').slice(0,16)}</td>
          </tr>
        );
      })}</tbody>
    </table>
  );
}

// ── ContextSizePanel ───────────────────────────────────────────────────────
function ContextSizePanel({ data, navigateToSession }) {
  if (!data) return <div className="muted">No data</div>;
  const { BarChart, Bar, XAxis, YAxis, Tooltip, Legend, ResponsiveContainer } = window.Recharts || {};
  if (!BarChart) return <div className="muted">Recharts not loaded</div>;
  const { distribution, big_sessions } = data;

  const chartData = (distribution || []).map(r => ({
    bucket: r.bucket,
    main:   (r.sessions || 0) - (r.subagent_sessions || 0),
    subagent: r.subagent_sessions || 0,
  }));

  return (
    <div>
      <ResponsiveContainer width="100%" height={180}>
        <BarChart data={chartData} margin={{ top: 4, right: 20, bottom: 4, left: 0 }}>
          <XAxis dataKey="bucket" />
          <YAxis />
          <Tooltip />
          <Legend />
          <Bar dataKey="main"     stackId="a" fill="#6baed6" name="main sessions" />
          <Bar dataKey="subagent" stackId="a" fill="#a78bfa" name="subagent sessions" />
        </BarChart>
      </ResponsiveContainer>
      {big_sessions && big_sessions.length > 0 && (
        <>
          <div style={{ marginTop: 12, fontWeight: 600, fontSize: '0.9em' }}>Sessions ≥200k tokens (top {big_sessions.length} by cost)</div>
          <table className="data-table" style={{ marginTop: 6 }}>
            <thead><tr><th>Session</th><th>Peak ctx</th><th>Cost</th><th>Turns</th><th>Subagent?</th></tr></thead>
            <tbody>{big_sessions.map((r, i) => (
              <tr key={i} style={{ cursor: 'pointer' }}
                  onClick={() => navigateToSession(r.project, r.session_id)}>
                <td style={{ fontFamily: 'monospace', fontSize: '0.85em' }}>{(r.session_id || '').slice(0,8)}</td>
                <td>{r.peak_ctx != null ? `${(r.peak_ctx/1000).toFixed(0)}k` : '—'}</td>
                <td>${(r.cost_usd || 0).toFixed(2)}</td>
                <td>{r.turn_count}</td>
                <td>{r.is_subagent ? 'yes' : 'no'}</td>
              </tr>
            ))}</tbody>
          </table>
        </>
      )}
    </div>
  );
}

// ── TopTurnsTable ─────────────────────────────────────────────────────────
function TopTurnsTable({ data, navigateToSession }) {
  const rows = data?.rows || [];
  if (rows.length === 0) return <div className="muted">No data</div>;
  const fmtK = n => n == null ? '—' : n >= 1000 ? `${(n/1000).toFixed(0)}k` : String(n);
  return (
    <table className="data-table">
      <thead>
        <tr>
          <th>Model</th><th>$</th><th>Input</th><th>Output</th>
          <th>CC</th><th>Cache-read</th><th>Tools</th><th>Session</th><th>Time</th>
        </tr>
      </thead>
      <tbody>{rows.map((r, i) => (
        <tr key={i} style={{ cursor: 'pointer' }}
            onClick={() => navigateToSession(r.project, r.session_id, r.entry_id)}>
          <td style={{ fontSize: '0.8em', maxWidth: 160, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{r.model || '—'}</td>
          <td>${(r.cost_usd || 0).toFixed(4)}</td>
          <td>{fmtK(r.input_tokens)}</td>
          <td>{fmtK(r.output_tokens)}</td>
          <td>{fmtK(r.cc_tokens)}</td>
          <td>{fmtK(r.cache_read_tokens)}</td>
          <td>{r.tool_use_count ?? '—'}</td>
          <td style={{ fontFamily: 'monospace', fontSize: '0.85em' }}>{(r.session_id || '').slice(0,8)}</td>
          <td style={{ whiteSpace: 'nowrap', fontSize: '0.85em' }}>{(r.ts || '').slice(0,16)}</td>
        </tr>
      ))}</tbody>
    </table>
  );
}

// ── CacheInvalidationPanel ────────────────────────────────────────────────
function CacheInvalidationPanel({ data }) {
  if (!data) return <div className="muted">No data</div>;
  const { threshold_p90, events } = data;
  if (!events || events.length === 0) return <div className="muted">No high-CC mid-session events in range</div>;
  return (
    <div>
      <div className="muted" style={{ marginBottom: 8, fontSize: '0.85em' }}>
        High-CC threshold: &gt;{(threshold_p90 || 0).toLocaleString()} tokens (your p90)
      </div>
      <table className="data-table">
        <thead><tr><th>Gap</th><th>CC type</th><th>Events</th><th>Cost</th></tr></thead>
        <tbody>{events.map((r, i) => (
          <tr key={i}>
            <td>{r.gap_bucket}</td>
            <td>{r.cc_type}</td>
            <td>{r.events}</td>
            <td>${(r.cost_usd || 0).toFixed(2)}</td>
          </tr>
        ))}</tbody>
      </table>
    </div>
  );
}

// ── HooksPanel ─────────────────────────────────────────────────────────────
function HooksPanel({ data }) {
  const rows = data?.rows || [];
  if (rows.length === 0) return <div className="muted">No hook data in range</div>;
  return (
    <table className="data-table">
      <thead><tr><th>Command</th><th>Invocations</th><th>Avg ms</th><th>Total sec</th></tr></thead>
      <tbody>{rows.map((r, i) => (
        <tr key={i}>
          <td style={{ fontFamily: 'monospace', fontSize: '0.85em', maxWidth: 300, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
              title={r.command}>{r.command || '—'}</td>
          <td>{r.invocations}</td>
          <td>{(r.avg_duration_ms||0).toFixed(0)}</td>
          <td>{(r.total_seconds||0).toFixed(1)}</td>
        </tr>
      ))}</tbody>
    </table>
  );
}

// ── SkillsPanel ────────────────────────────────────────────────────────────
function SkillsPanel({ data }) {
  const rows = data?.rows || [];
  if (rows.length === 0) return <div className="muted">No skill invocations in range</div>;
  return (
    <table className="data-table">
      <thead><tr>
        <th>Skill name</th>
        <th className="num">Invocations</th>
        <th className="num">Sessions</th>
      </tr></thead>
      <tbody>{rows.map((r, i) => (
        <tr key={i}>
          <td style={{ fontFamily: 'monospace', fontSize: '0.85em' }}>{r.skill_name || '—'}</td>
          <td className="num">{fmtNum(r.invocations || 0)}</td>
          <td className="num">{fmtNum(r.sessions || 0)}</td>
        </tr>
      ))}</tbody>
    </table>
  );
}

// ── BashPanel ──────────────────────────────────────────────────────────────
function BashPanel({ data, navigateToSession }) {
  const longest = data?.longest || [];
  const mostRepeated = data?.most_repeated || [];
  if (longest.length === 0 && mostRepeated.length === 0) return <div className="muted">No Bash calls in range</div>;
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {longest.length > 0 && (
        <div>
          <div style={{ fontSize: 11, color: 'var(--muted)', marginBottom: 6 }}>Longest Commands</div>
          <table className="data-table">
            <thead><tr>
              <th>Preview</th>
              <th className="num">Size</th>
              <th>Session</th>
              <th>Time</th>
            </tr></thead>
            <tbody>{longest.map((r, i) => (
              <tr key={i} style={{ cursor: navigateToSession ? 'pointer' : undefined }}
                  onClick={() => navigateToSession && navigateToSession(r.project, r.session_id)}>
                <td style={{ fontFamily: 'monospace', fontSize: '0.85em', maxWidth: 340, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
                    title={r.cmd_preview}>{r.cmd_preview || '—'}</td>
                <td className="num">{fmtNum(r.cmd_chars || 0)}</td>
                <td className="link" style={{ fontFamily: 'monospace', fontSize: '0.85em' }}>{(r.session_id || '').slice(0, 8)}</td>
                <td>{fmtDate(r.ts)}</td>
              </tr>
            ))}</tbody>
          </table>
        </div>
      )}
      {mostRepeated.length > 0 && (
        <div>
          <div style={{ fontSize: 11, color: 'var(--muted)', marginBottom: 6 }}>Most Repeated (by first token)</div>
          <table className="data-table">
            <thead><tr>
              <th>Command</th>
              <th className="num">Calls</th>
              <th className="num">Avg result chars</th>
            </tr></thead>
            <tbody>{mostRepeated.map((r, i) => (
              <tr key={i}>
                <td style={{ fontFamily: 'monospace', fontSize: '0.85em' }}>{r.cmd_head || '—'}</td>
                <td className="num">{fmtNum(r.calls || 0)}</td>
                <td className="num">{fmtNum(r.avg_result_chars || 0)}</td>
              </tr>
            ))}</tbody>
          </table>
        </div>
      )}
    </div>
  );
}

// ── ReadSizesPanel ─────────────────────────────────────────────────────────
function ReadSizesPanel({ data, navigateToSession }) {
  const rows = data?.rows || [];
  if (rows.length === 0) return <div className="muted">No Read calls in range</div>;
  return (
    <table className="data-table">
      <thead><tr>
        <th>File</th>
        <th className="num">Size (chars)</th>
        <th>Session</th>
        <th>Time</th>
      </tr></thead>
      <tbody>{rows.map((r, i) => (
        <tr key={i} style={{ cursor: navigateToSession ? 'pointer' : undefined }}
            onClick={() => navigateToSession && navigateToSession(r.project, r.session_id)}>
          <td style={{ fontFamily: 'monospace', fontSize: '0.85em', maxWidth: 300, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
              title={r.file_path}>{r.file_path || '—'}</td>
          <td className="num">{fmtNum(r.result_chars || 0)}</td>
          <td className="link" style={{ fontFamily: 'monospace', fontSize: '0.85em' }}>{(r.session_id || '').slice(0, 8)}</td>
          <td>{fmtDate(r.ts)}</td>
        </tr>
      ))}</tbody>
    </table>
  );
}

// ── McpToolsPanel ──────────────────────────────────────────────────────────
function McpToolsPanel({ data }) {
  const rows = data?.rows || [];
  if (rows.length === 0) return <div className="muted">No MCP tool calls in range</div>;
  return (
    <table className="data-table">
      <thead><tr>
        <th>Tool</th>
        <th>Server</th>
        <th className="num">Calls</th>
        <th className="num">Avg chars</th>
        <th className="num">Max chars</th>
        <th className="num">Total Mchars</th>
      </tr></thead>
      <tbody>{rows.map((r, i) => (
        <tr key={i}>
          <td style={{ fontFamily: 'monospace', fontSize: '0.85em', maxWidth: 260, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
              title={r.tool_name}>{r.tool_name || '—'}</td>
          <td>{r.mcp_server || '—'}</td>
          <td className="num">{fmtNum(r.calls || 0)}</td>
          <td className="num">{fmtNum(r.avg_result_chars || 0)}</td>
          <td className="num">{fmtNum(r.max_result_chars || 0)}</td>
          <td className="num">{(r.total_mchars || 0).toFixed(2)}</td>
        </tr>
      ))}</tbody>
    </table>
  );
}

// ── HourOfDayChart ─────────────────────────────────────────────────────────
function HourOfDayChart({ data }) {
  const rows = Array.isArray(data) ? data : [];
  if (rows.length === 0) return <div className="muted">No data</div>;
  const { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer } = window.Recharts || {};
  if (!BarChart) return <div className="muted">Recharts not loaded</div>;
  return (
    <ResponsiveContainer width="100%" height={160}>
      <BarChart data={rows} margin={{ top: 4, right: 20, bottom: 4, left: 0 }}>
        <XAxis dataKey="hour" />
        <YAxis tickFormatter={v => `$${v}`} />
        <Tooltip formatter={v => [`$${v}`, 'cost']} labelFormatter={h => `Hour ${h}:00`} />
        <Bar dataKey="cost_usd" fill="#6baed6" name="cost" />
      </BarChart>
    </ResponsiveContainer>
  );
}

// ── CompactionsPanel ───────────────────────────────────────────────────────
function CompactionsPanel({ data }) {
  if (!data) return <div className="muted">No data</div>;
  const { count, events } = data;
  if (!events || events.length === 0)
    return <div className="muted">No compaction events in this window</div>;
  return (
    <div>
      <div style={{ marginBottom: 8, fontSize: '0.9em' }}>{count} compaction event{count !== 1 ? 's' : ''}</div>
      <table className="data-table">
        <thead><tr><th>Session</th><th>Gap</th><th>Next turn $</th><th>Summary preview</th></tr></thead>
        <tbody>{events.map((r, i) => (
          <tr key={i}>
            <td style={{ fontFamily: 'monospace', fontSize: '0.85em' }}>{(r.session_id||'').slice(0,8)}</td>
            <td>{r.gap_min != null ? `${r.gap_min}m` : '—'}</td>
            <td>${(r.next_turn_cost||0).toFixed(4)}</td>
            <td style={{ maxWidth: 300, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
                title={r.summary_preview}>{r.summary_preview||'—'}</td>
          </tr>
        ))}</tbody>
      </table>
    </div>
  );
}

// ── GlobalDashboard ────────────────────────────────────────────────────────
function GlobalDashboard({ timeRange, projects, navigateToSession }) {
  const urls = useMemo(() => ({
    baseline:            buildUrl('/api/dashboard/baseline', timeRange),
    summary:             buildUrl('/api/dashboard/summary', timeRange),
    daily:               buildUrl('/api/dashboard/daily', timeRange),
    models:              buildUrl('/api/dashboard/models', timeRange),
    cache:               buildUrl('/api/dashboard/cache', timeRange),
    agents:              buildUrl('/api/dashboard/agents', timeRange),
    top:                 buildUrl('/api/dashboard/top-sessions', timeRange),
    dist:                buildUrl('/api/dashboard/session-distribution', timeRange),
    files:               buildUrl('/api/dashboard/file-hotspots', timeRange),
    errors:              buildUrl('/api/dashboard/errors', timeRange),
    tokenStreams:        buildUrl('/api/dashboard/token-streams', timeRange),
    artifactWrites:      buildUrl('/api/dashboard/artifacts', timeRange, { kind: 'write' }),
    artifactAgents:      buildUrl('/api/dashboard/artifacts', timeRange, { kind: 'agent' }),
    artifactToolResults: buildUrl('/api/dashboard/artifacts', timeRange, { kind: 'tool_result' }),
    ctxSize:             buildUrl('/api/dashboard/context-size', timeRange),
    topTurns:            buildUrl('/api/dashboard/top-turns', timeRange),
    twoRegime:           buildUrl('/api/dashboard/two-regime', timeRange),
    firstTurnCc:         buildUrl('/api/dashboard/first-turn-cc', timeRange),
    cacheInval:          buildUrl('/api/dashboard/cache-invalidation', timeRange),
    compactions:         buildUrl('/api/dashboard/compactions', timeRange),
    hourOfDay:           buildUrl('/api/dashboard/hour-of-day', timeRange),
    hooks:               buildUrl('/api/dashboard/hooks', timeRange),
    mcpTools:            buildUrl('/api/dashboard/mcp-tools', timeRange),
    readSizes:           buildUrl('/api/dashboard/read-sizes', timeRange),
    bash:                buildUrl('/api/dashboard/bash', timeRange),
    skills:              buildUrl('/api/dashboard/skills', timeRange),
  }), [timeRange]);
  const summary            = usePanel(urls.summary);
  const daily              = usePanel(urls.daily);
  const models             = usePanel(urls.models);
  const cache              = usePanel(urls.cache);
  const agents             = usePanel(urls.agents);
  const top                = usePanel(urls.top);
  const dist               = usePanel(urls.dist);
  const files              = usePanel(urls.files);
  const errors             = usePanel(urls.errors);
  const tokenStreams        = usePanel(urls.tokenStreams);
  const artifactWrites      = usePanel(urls.artifactWrites);
  const artifactAgents      = usePanel(urls.artifactAgents);
  const artifactToolResults = usePanel(urls.artifactToolResults);
  const ctxSize             = usePanel(urls.ctxSize);
  const topTurns            = usePanel(urls.topTurns);
  const twoRegime           = usePanel(urls.twoRegime);
  const firstTurnCc         = usePanel(urls.firstTurnCc);
  const cacheInval          = usePanel(urls.cacheInval);
  const compactions         = usePanel(urls.compactions);
  const hourOfDay           = usePanel(urls.hourOfDay);
  const hooks               = usePanel(urls.hooks);
  const mcpTools            = usePanel(urls.mcpTools);
  const readSizes           = usePanel(urls.readSizes);
  const bash                = usePanel(urls.bash);
  const skills              = usePanel(urls.skills);

  const initSub = (new URLSearchParams(window.location.search)).get('sub') === 'outliers' ? 'outliers' : 'overview';
  const [subTab, setSubTab] = React.useState(initSub);

  React.useEffect(() => {
    const p = new URLSearchParams(window.location.search);
    if (subTab !== 'overview') p.set('sub', subTab); else p.delete('sub');
    const qs = p.toString();
    window.history.replaceState(null, '', qs ? '?' + qs : window.location.pathname);
  }, [subTab]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <BaselineBar url={urls.baseline} />
      <div className="sort-group subtabs">
        <button className={`sort-btn ${subTab === 'overview' ? 'active' : ''}`}
                onClick={() => setSubTab('overview')}>Overview</button>
        <button className={`sort-btn ${subTab === 'outliers' ? 'active' : ''}`}
                onClick={() => setSubTab('outliers')}>Outliers</button>
      </div>
      {subTab === 'overview' ? (
        <>
          {/* OVERVIEW: general info, where money went */}
          <Panel title="Summary" meta={timeRange === 'all' ? 'all time' : `last ${timeRange}`} loading={summary.loading} err={summary.err}>
            <SummaryBar data={summary.data} />
          </Panel>
          <Panel title="Daily Spend by Model" loading={daily.loading} err={daily.err}>
            <DailySpendChart data={daily.data} />
          </Panel>
          <Panel title="Sessions/Week + $/Session" meta="volume vs per-session cost · main sessions only" loading={twoRegime.loading} err={twoRegime.err}>
            <TwoRegimeChart data={twoRegime.data} />
          </Panel>
          <Panel title="Token-type Cost Split" meta="main-chain vs sidechain · derived from model_pricing" loading={tokenStreams.loading} err={tokenStreams.err}>
            <TokenStreamsPanel data={tokenStreams.data} />
          </Panel>
          <Panel title="First-turn Cache-Creation Distribution" meta="system-prompt size proxy" loading={firstTurnCc.loading} err={firstTurnCc.err}>
            <FirstTurnCcChart data={firstTurnCc.data} />
          </Panel>
          <Panel title="Model Breakdown" loading={models.loading} err={models.err}>
            <ModelBreakdownTable data={models.data} />
          </Panel>
          <Panel title="Errors" loading={errors.loading} err={errors.err}>
            <ErrorSummaryPanel data={errors.data} />
          </Panel>
        </>
      ) : (
        <>
          {/* OUTLIERS: actionable, what to change */}
          <Panel title="Top 1% Most-Expensive Turns" meta="top 30 by cost · click to open session" loading={topTurns.loading} err={topTurns.err}>
            <TopTurnsTable data={topTurns.data} navigateToSession={navigateToSession} />
          </Panel>
          <Panel title="Top Sessions" meta="by cost · click a row to open in Transcripts" loading={top.loading} err={top.err}>
            <TopSessionsTable data={top.data} projects={projects} navigateToSession={navigateToSession} />
          </Panel>
          <Panel title="Context Size Distribution" meta="peak tokens per session · click 200k+ rows to open transcript" loading={ctxSize.loading} err={ctxSize.err}>
            <ContextSizePanel data={ctxSize.data} navigateToSession={navigateToSession} />
          </Panel>
          <Panel title="Cache Invalidation Events" meta="mid-session high-CC turns by gap × type" loading={cacheInval.loading} err={cacheInval.err}>
            <CacheInvalidationPanel data={cacheInval.data} />
          </Panel>
          <Panel title="Compaction Events" loading={compactions.loading} err={compactions.err}>
            <CompactionsPanel data={compactions.data} />
          </Panel>
          <Panel title="Hour-of-Day Cost" loading={hourOfDay.loading} err={hourOfDay.err}>
            <HourOfDayChart data={hourOfDay.data} />
          </Panel>
          <Panel title="Artifact Leaderboard: Large Writes" meta="top 30 by content size" loading={artifactWrites.loading} err={artifactWrites.err}>
            <ArtifactLeaderboard data={artifactWrites.data} kind="write" navigateToSession={navigateToSession} />
          </Panel>
          <Panel title="Artifact Leaderboard: Agent Prompts" meta="top 30 by prompt size" loading={artifactAgents.loading} err={artifactAgents.err}>
            <ArtifactLeaderboard data={artifactAgents.data} kind="agent" navigateToSession={navigateToSession} />
          </Panel>
          <Panel title="Artifact Leaderboard: Tool Results" meta="top 30 by result size" loading={artifactToolResults.loading} err={artifactToolResults.err}>
            <ArtifactLeaderboard data={artifactToolResults.data} kind="tool_result" navigateToSession={navigateToSession} />
          </Panel>
          <Panel title="Top Reads by Size" loading={readSizes.loading} err={readSizes.err}>
            <ReadSizesPanel data={readSizes.data} navigateToSession={navigateToSession} />
          </Panel>
          <Panel title="File Hotspots" meta="top 30 by distinct session reads" loading={files.loading} err={files.err}>
            <FileHotspotsTable data={files.data} />
          </Panel>
          <Panel title="Bash Leaderboards" loading={bash.loading} err={bash.err}>
            <BashPanel data={bash.data} navigateToSession={navigateToSession} />
          </Panel>
          <Panel title="MCP Tool Result Sizes" loading={mcpTools.loading} err={mcpTools.err}>
            <McpToolsPanel data={mcpTools.data} />
          </Panel>
          <Panel title="Hook Frequency & Duration" meta="hook output size not stored in DB" loading={hooks.loading} err={hooks.err}>
            <HooksPanel data={hooks.data} />
          </Panel>
          <Panel title="Skill Invocation Stats" loading={skills.loading} err={skills.err}>
            <SkillsPanel data={skills.data} />
          </Panel>
          <Panel title="Agent Model Usage" loading={agents.loading} err={agents.err}>
            <AgentModelPanel data={agents.data} />
          </Panel>
          <Panel title="Cache Health" loading={cache.loading} err={cache.err}>
            <CacheHealthPanel data={cache.data} />
          </Panel>
          <Panel title="Session Distribution" meta="by turn count" loading={dist.loading} err={dist.err}>
            <SessionDistributionTable data={dist.data} />
          </Panel>
        </>
      )}
    </div>
  );
}

// ── DashboardView ──────────────────────────────────────────────────────────
function DashboardView({ timeRange, projects, navigateToSession }) {
  return (
    <div className="dashboard">
      <GlobalDashboard timeRange={timeRange} projects={projects} navigateToSession={navigateToSession} />
    </div>
  );
}

// ── TranscriptView ─────────────────────────────────────────────────────────
function TranscriptView({ selectedProject, initialSessionId, initialSort, targetEntry, clearTargetEntry }) {
  const [sessions,    setSessions]    = useState([]);
  const [sessionId,   setSessionId]   = useState(initialSessionId || '');
  const [sortBy,      setSortBy]      = useState(initialSort || 'date');
  const [timeline,    setTimeline]    = useState(null);
  const [loading,     setLoading]     = useState(false);
  const [err,         setErr]         = useState(null);
  const [agentCache,  setAgentCache]  = useState({});

  // Preserve URL session on first project load; cleared after first use
  const initSessionRef = useRef(initialSessionId || null);

  // Sync session + sort + entry back to URL (preserve tab/range/project already set by App)
  useEffect(() => {
    const p = new URLSearchParams(window.location.search);
    if (sessionId) p.set('session', sessionId); else p.delete('session');
    if (sortBy !== 'date') p.set('sort', sortBy); else p.delete('sort');
    if (targetEntry) p.set('entry', targetEntry); else p.delete('entry');
    const qs = p.toString();
    window.history.replaceState(null, '', qs ? '?' + qs : window.location.pathname);
  }, [sessionId, sortBy, targetEntry]);

  // When project changes (or is cleared), load its sessions
  useEffect(() => {
    setTimeline(null);
    if (!selectedProject) {
      setSessions([]);
      setSessionId('');
      return;
    }
    setSessions([]);
    const preserveSession = initSessionRef.current;
    if (!preserveSession) setSessionId('');
    fetch(`/api/sessions?project=${encodeURIComponent(selectedProject)}`)
      .then(r => r.json())
      .then(data => {
        setSessions(data);
        if (preserveSession) {
          initSessionRef.current = null;
          if (!data.find(s => s.id === preserveSession) && data.length) setSessionId(data[0].id);
          else setSessionId(preserveSession);
        } else {
          if (data.length) setSessionId(data[0].id);
        }
      })
      .catch(() => {});
  }, [selectedProject]);

  const sortedSessions = useMemo(() => {
    const s = [...sessions];
    if (sortBy === 'cost') s.sort((a, b) => (b.costUsd || 0) - (a.costUsd || 0));
    else s.sort((a, b) => new Date(b.lastActive) - new Date(a.lastActive));
    return s;
  }, [sessions, sortBy]);

  // Preload all subagent data in parallel after timeline loads
  useEffect(() => {
    if (!timeline || !sessionId) return;
    setAgentCache({});
    const agentIds = [];
    for (const item of timeline) {
      if (item.kind === 'assistant') {
        for (const tu of item.tool_uses || []) {
          if (tu.agent_id) agentIds.push(tu.agent_id);
        }
      }
    }
    if (agentIds.length === 0) return;
    for (const agentId of agentIds) {
      fetch(`/api/subagent?session=${encodeURIComponent(sessionId)}&agent=${encodeURIComponent(agentId)}`)
        .then(r => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
        .then(data => setAgentCache(prev => ({ ...prev, [agentId]: data.entries || [] })))
        .catch(() => {});
    }
  }, [timeline, sessionId]);

  // Scroll to target entry after timeline loads
  useEffect(() => {
    if (!timeline || !targetEntry) return;
    requestAnimationFrame(() => {
      const el = document.getElementById(`entry-${targetEntry}`);
      if (el) {
        el.scrollIntoView({ behavior: 'smooth', block: 'center' });
        el.style.outline = '2px solid var(--accent)';
        setTimeout(() => { el.style.outline = ''; }, 2000);
        if (clearTargetEntry) clearTargetEntry();
      }
    });
  }, [timeline, targetEntry]);

  const load = useCallback(async () => {
    if (!selectedProject || !sessionId) return;
    setLoading(true); setErr(null); setTimeline(null); setAgentCache({});
    try {
      const r = await fetch(`/api/transcript?project=${encodeURIComponent(selectedProject)}&session=${encodeURIComponent(sessionId)}`);
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      const data = await r.json();
      setTimeline(data.entries || []);
    } catch (e) { setErr(e.message); }
    setLoading(false);
  }, [selectedProject, sessionId]);

  useEffect(() => { if (sessionId) load(); }, [sessionId, selectedProject]);

  const apiItems = useMemo(() =>
    (timeline || []).flatMap((item, idx) => item.kind === 'assistant' ? [{ ...item, timelineIdx: idx }] : []),
    [timeline]);
  const maxCost  = useMemo(() => Math.max(1e-9, apiItems.reduce((s, i) => {
    const subTotal = (i.tool_uses || []).reduce((a, tu) => a + (tu.subagent_cost_usd || 0), 0);
    return s + (i.cost_usd || 0) + subTotal;
  }, 0)), [apiItems]);

  if (!selectedProject) {
    return (
      <>
        <div className="timeline">
          <div className="empty">Select a project from the header dropdown to view transcripts.</div>
        </div>
      </>
    );
  }

  return (
    <>
      <div className="header" style={{ borderTop: '1px solid var(--border)' }}>
        <label>Sort</label>
        <div className="sort-group">
          <button className={`sort-btn ${sortBy === 'date' ? 'active' : ''}`} onClick={() => setSortBy('date')}>date</button>
          <button className={`sort-btn ${sortBy === 'cost' ? 'active' : ''}`} onClick={() => setSortBy('cost')}>cost</button>
        </div>

        <label>Session</label>
        <select value={sessionId} onChange={e => setSessionId(e.target.value)} style={{ minWidth: 260, maxWidth: 420 }}>
          {sortedSessions.map(s => (
            <option key={s.id} value={s.id}>
              {s.hasSubagents ? '🤖 ' : ''}{fmtDate(s.lastActive)} — {fmtCost(s.costUsd)} [{s.id.slice(0, 8)}]
            </option>
          ))}
        </select>

        <button className="btn" onClick={load} disabled={loading || !sessionId}>
          {loading ? 'Loading…' : 'Reload'}
        </button>

        {err && <span className="err">{err}</span>}
      </div>

      {timeline && <SessionTotals timeline={timeline} />}
      {timeline && <CostChart apiItems={apiItems} />}
      {timeline && <ActivityBreakdown timeline={timeline} />}

      <div className="timeline">
        {!timeline && !loading && <div className="empty">Select a session above</div>}
        {loading && <div className="empty">Loading transcript…</div>}
        {timeline && timeline.map((item, idx) => renderItem(item, idx, maxCost, sessionId, agentCache))}
      </div>

      <div className="legend">
        <span>Token legend:</span>
        {[['var(--tok-input)','input'],['var(--tok-cr)','cache read'],['var(--tok-cw)','cache write'],['var(--tok-out)','output'],['var(--tok-subagent)','subagent']].map(([c,l]) => (
          <span key={l} className="leg-item">
            <span className="leg-swatch" style={{ background: c }} /> {l}
          </span>
        ))}
        <span style={{ marginLeft: 10, color: 'var(--muted)', fontStyle: 'italic' }}>
          tok bar: 100% = 1M tokens · cost bar: 100% = total session cost · cost from DB
        </span>
      </div>
    </>
  );
}

// ── App ────────────────────────────────────────────────────────────────────
function App() {
  const initUrl = useMemo(() => getUrlParams(), []);
  const [projects,        setProjects]        = useState([]);
  const [selectedProject, setSelectedProject] = useState(initUrl.project);
  const [activeTab,       setActiveTab]       = useState(initUrl.tab === 'transcripts' ? 'transcripts' : 'dashboard');
  const [timeRange,       setTimeRange]       = useState(['7d','30d','90d','all'].includes(initUrl.range) ? initUrl.range : '30d');
  const [transcriptSessionId, setTranscriptSessionId] = useState(initUrl.session);
  const [targetEntry,     setTargetEntry]     = useState(initUrl.entry);
  const [err,             setErr]             = useState(null);

  // Sync top-level state to URL
  useEffect(() => {
    const p = new URLSearchParams(window.location.search);
    if (activeTab !== 'dashboard') p.set('tab', activeTab); else p.delete('tab');
    if (timeRange !== '30d') p.set('range', timeRange); else p.delete('range');
    if (activeTab === 'transcripts' && selectedProject) p.set('project', selectedProject);
    else p.delete('project');
    // session/sort are managed by TranscriptView — leave them alone here
    const qs = p.toString();
    window.history.replaceState(null, '', qs ? '?' + qs : window.location.pathname);
  }, [activeTab, timeRange, selectedProject]);

  useEffect(() => {
    fetch('/api/projects')
      .then(r => r.json())
      .then(data => { setProjects(data); })
      .catch(() => setErr('Cannot reach server — is claude-code-transcripts-serve running? (cargo run --features serve --bin claude-code-transcripts-serve)'));
  }, []);

  const navigateToSession = useCallback((project, sessionId, entryId) => {
    setSelectedProject(project || '');
    setTranscriptSessionId(sessionId || '');
    setTargetEntry(entryId != null ? String(entryId) : '');
    setActiveTab('transcripts');
  }, []);

  const TIME_RANGES = ['7d', '30d', '90d', 'all'];

  return (
    <>
      <div className="header">
        <div className="tab-nav">
          <button
            className={`tab-btn ${activeTab === 'dashboard' ? 'active' : ''}`}
            onClick={() => setActiveTab('dashboard')}
          >Dashboard</button>
          <button
            className={`tab-btn ${activeTab === 'transcripts' ? 'active' : ''}`}
            onClick={() => setActiveTab('transcripts')}
          >Transcripts</button>
        </div>

        {activeTab === 'dashboard' && (
          <>
            <span className="sep">·</span>
            <div className="sort-group">
              {TIME_RANGES.map(r => (
                <button
                  key={r}
                  className={`sort-btn ${timeRange === r ? 'active' : ''}`}
                  onClick={() => setTimeRange(r)}
                >{r === 'all' ? 'All' : r}</button>
              ))}
            </div>
          </>
        )}

        {activeTab === 'transcripts' && (
          <>
            <span className="sep">·</span>
            <label>Project</label>
            <select
              value={selectedProject}
              onChange={e => setSelectedProject(e.target.value)}
              style={{ minWidth: 200, maxWidth: 360 }}
            >
              <option value="">All Projects</option>
              {projects.map(p => (
                <option key={p.key} value={p.key}>{p.display} ({p.sessionCount})</option>
              ))}
            </select>
          </>
        )}

        {err && <span className="err">{err}</span>}
      </div>

      {activeTab === 'dashboard'
        ? <DashboardView
            timeRange={timeRange}
            projects={projects}
            navigateToSession={navigateToSession}
          />
        : <TranscriptView
            selectedProject={selectedProject}
            initialSessionId={transcriptSessionId}
            initialSort={initUrl.sort}
            targetEntry={targetEntry}
            clearTargetEntry={() => setTargetEntry('')}
          />
      }
    </>
  );
}

export {
  App,
  DashboardView,
  TranscriptView,
  SessionTotals,
  CostChart,
  ActivityBreakdown,
  renderItem,
  fmtCost,
  fmtDate,
};
