// MIPI RFFEのコマンドを読み解く拡張(規格の読み解きだけ). 計算のモード(`= `)の中でも, ふだんの検索でも使える.
//   mipi 01100101110010011100000       ビット列(最後のBまで) -> フレームとビットの分解
//   mipi sa=6 addr=1C data=38           値(16進) -> ビット列. 順番は自由. cmd=を省くと, 使えるコマンドを全部出す
//   mipi                                クリップボードのビット列か表(SCLKの列とデータの列)を読む. なければコマンドの一覧
// Masked Writeのコマンドの8ビットは公開されていないので持たない. config.tomlの[ext.mipi] masked_write に書くと使える.
export const meta = { title: "MIPI RFFE", keyword: "mipi", clipboard: true, calc: true };

const REG_NAMES = { 0x1c: "PM_TRIG", 0x1d: "PRODUCT_ID", 0x1e: "MANUFACTURER_ID", 0x1f: "MAN_USID" };
const PWR_MODES = ["Active", "Startup", "Low Power", "Reserved"];
const CMDS = [
  { key: "w0", title: "Register 0 Write", need: ["sa", "data"] },
  { key: "w", title: "Register Write", need: ["sa", "addr", "data"] },
  { key: "r", title: "Register Read", need: ["sa", "addr"] },
  { key: "ew", title: "Extended Register Write", need: ["sa", "addr", "data"] },
  { key: "er", title: "Extended Register Read", need: ["sa", "addr"] },
  { key: "ewl", title: "Extended Register Write Long", need: ["sa", "addr", "data"] },
  { key: "erl", title: "Extended Register Read Long", need: ["sa", "addr"] },
  { key: "mw", title: "Masked Write", need: ["sa", "addr", "mask", "data"] },
];
const KEYS = { cmd: "Command", sa: "Slave address (hex, 0 to F)", addr: "Register address (hex)", data: "Data (hex, one or more bytes)", mask: "Mask (hex). Bits that are 0 are written", bytes: "Number of bytes to read" };

const bin = (n, w) => n.toString(2).padStart(w, "0");
const hex = (n, w) => "0x" + n.toString(16).toUpperCase().padStart(w || 1, "0");
const ones = (s) => s.split("").filter((c) => c === "1").length;
// 奇数パリティ: パリティを含めたフレームの1の数が奇数になる値(コマンドフレームはSAも含む13ビット)
const parityOf = (s) => (ones(s) % 2 === 0 ? "1" : "0");
const regLabel = (a, w) => hex(a, w) + (REG_NAMES[a] ? " " + REG_NAMES[a] : "");
const pmTrig = (d) => `${PWR_MODES[d >> 6]}, mask ${bin((d >> 3) & 7, 3)}, trig ${bin(d & 7, 3)}`;
const maskCmd = (ctx) => {
  const v = String((ctx.options || {}).masked_write || "").replace(/[\s_]/g, "");
  return /^[01]{8}$/.test(v) ? v : "";
};
const titleOf = (key) => CMDS.find((c) => c.key === key).title;

// コマンドフレームの8ビットから, 種類とこのあとの並びを決める
function plan(body, mask) {
  const seg = (label, bits, kind) => ({ label, bits, kind });
  if (mask && body === mask) return { key: "mw", segs: [seg("CMD", body, "cmd")], frames: ["addr", "mask", "data", "B"] };
  if (body[0] === "1") return { key: "w0", segs: [seg("CMD", "1", "cmd"), seg("DATA", body.slice(1), "data")], frames: ["B"], addr: 0, data7: parseInt(body.slice(1), 2) };
  if (body.startsWith("010") || body.startsWith("011")) {
    const read = body[2] === "1";
    const p = { key: read ? "r" : "w", segs: [seg("CMD", body.slice(0, 3), "cmd"), seg("ADDR", body.slice(3), "addr")], addr: parseInt(body.slice(3), 2) };
    p.frames = read ? ["B", "data", "B"] : ["data", "B"];
    return p;
  }
  const ext = body.startsWith("0000") || body.startsWith("0010");
  const long = body.startsWith("00110") || body.startsWith("00111");
  if (!ext && !long) return null;
  const cut = ext ? 4 : 5;
  const read = ext ? body[2] === "1" : body[4] === "1";
  const n = parseInt(body.slice(cut), 2) + 1;
  const data = new Array(n).fill("data");
  const addr = ext ? ["addr"] : ["addr", "addr"];
  return {
    key: (read ? "er" : "ew") + (long ? "l" : ""),
    segs: [seg("CMD", body.slice(0, cut), "cmd"), seg("BC", body.slice(cut), "cmd")],
    bytes: n,
    long,
    frames: read ? [...addr, "B", ...data, "B"] : [...addr, ...data, "B"],
  };
}

const LABELS = { addr: "ADDRESS", mask: "MASK", data: "DATA" };

// ビット列を規格どおりに分ける. framesは{kind, segs, bits, par, want}. 足りない, 余る, Bが0でないはerrorsに入れる
function decode(bits, mask) {
  const frames = [];
  const errors = [];
  const r = { frames, errors, data: [], mode: "", sa: "" };
  if (bits.length < 13) {
    errors.push({ title: `${13 - bits.length} more bits for the command frame`, sub: `The command frame is 13 bits (SA 4, command 8, parity 1). ${bits.length} typed` });
    if (bits.length >= 4) r.sa = hex(parseInt(bits.slice(0, 4), 2));
    return r;
  }
  const frame = (kind, segs, s) => {
    const body = s.slice(0, -1);
    const p = s.slice(-1);
    const want = parityOf(body);
    frames.push({ kind, segs: [...segs, { label: "P", bits: p, kind: "p" }], bits: s, ok: p === want, want });
  };
  const sab = bits.slice(0, 4);
  r.sa = hex(parseInt(sab, 2));
  const pl = plan(bits.slice(4, 12), mask);
  if (!pl) {
    frame("cmd", [{ label: "SA", bits: sab, kind: "sa" }, { label: "CMD", bits: bits.slice(4, 12), kind: "cmd" }], bits.slice(0, 13));
    errors.push({ title: "Unknown command", sub: `Command bits ${bits.slice(4, 12)}` });
    return r;
  }
  r.key = pl.key;
  r.mode = titleOf(pl.key);
  r.bytes = pl.bytes;
  frame("cmd", [{ label: "SA", bits: sab, kind: "sa" }, ...pl.segs], bits.slice(0, 13));
  if (pl.data7 !== undefined) r.data.push(pl.data7);
  let addrBits = pl.addr === undefined ? "" : null;
  if (pl.addr !== undefined) r.addr = pl.addr;
  const total = 13 + pl.frames.reduce((n, f) => n + (f === "B" ? 1 : 9), 0);
  r.total = total;
  let pos = 13;
  // 読みは, コマンドの側(データの前のBまで)だけでもよい
  const half = 13 + pl.frames.slice(0, pl.frames.indexOf("B") + 1).reduce((n, f) => n + (f === "B" ? 1 : 9), 0);
  const commandOnly = half < total && bits.length === half;
  for (const f of pl.frames) {
    const n = f === "B" ? 1 : 9;
    if (pos + n > bits.length) break;
    const s = bits.substr(pos, n);
    pos += n;
    if (f === "B") {
      frames.push({ kind: "b", segs: [{ label: "B", bits: s, kind: "b" }], bits: s, ok: s === "0" });
      if (s !== "0") errors.push({ title: "B must be 0", sub: "The bus park cycle is 0" });
      continue;
    }
    frame(f, [{ label: LABELS[f], bits: s.slice(0, 8), kind: f }], s);
    const v = parseInt(s.slice(0, 8), 2);
    frames[frames.length - 1].value = v;
    if (f === "addr") addrBits += s.slice(0, 8);
    else if (f === "mask") r.mask = v;
    else r.data.push(v);
  }
  if (addrBits && (!pl.long || addrBits.length === 16)) r.addr = parseInt(addrBits, 2);
  r.used = pos;
  if (commandOnly) r.commandOnly = true;
  else if (bits.length < total) {
    const left = total - bits.length;
    errors.push({ title: `${left} more bit${left > 1 ? "s" : ""}${left === 1 ? " (the last B)" : ""}`, sub: `${r.mode} is ${total} bits. ${bits.length} typed`, short: true });
  } else if (bits.length > total) errors.push({ title: `${bits.length - total} extra bit${bits.length - total > 1 ? "s" : ""}`, sub: `${r.mode} is ${total} bits. ${bits.length} typed` });
  return r;
}

const addrText = (r) => (r.addr === undefined ? "" : regLabel(r.addr, r.key === "ewl" || r.key === "erl" ? 4 : 2));
const isRead = (r) => /Read/.test(r.mode);
function describe(r) {
  let s = `SA ${r.sa}`;
  if (r.addr !== undefined) s += ` · ${addrText(r)}`;
  if (r.mask !== undefined) s += ` · mask ${bin(r.mask, 8)}`;
  if (r.data.length) s += ` ${isRead(r) ? "→" : "←"} ${r.data.map((v) => hex(v, 2)).join(" ")}`;
  else if (r.bytes && isRead(r)) s += ` · ${r.bytes} byte${r.bytes > 1 ? "s" : ""}`;
  if (r.addr === 0x1c && r.data.length === 1 && r.mask === undefined) s += ` · ${pmTrig(r.data[0])}`;
  return s;
}
const grouped = (r) => r.frames.map((f) => f.segs.map((s) => s.bits).join(" ")).join("  ");
const parityOk = (r) => r.frames.every((f) => f.ok);

// 帯(1行に入る幅で分ける). 初めの行はSSC, コマンド, アドレスと, データ3つまで. 次の行からはデータ6つずつ
function bandRows(r, copy) {
  const rows = [];
  let cur = [{ label: "SSC", bits: "", kind: "b" }];
  let data = 0;
  let max = 3;
  for (const f of r.frames) {
    if (f.kind === "data" && data === max) {
      rows.push(cur);
      cur = [];
      data = 0;
      max = 6;
    }
    if (f.kind === "data") data++;
    f.segs.forEach((s, i) => cur.push({ label: s.label, bits: s.bits, kind: s.kind, gap: i === 0 && cur.length > 0, bad: !f.ok }));
  }
  rows.push(cur);
  return rows.map((band) => ({ title: band.map((s) => s.bits || s.label).join(" "), subtitle: "", copy, band }));
}

// 1つのコマンドの分解: 帯, まとめ, 誤り, フレームごとの行
function breakdown(r, copy, from) {
  if (!r.frames.length) return r.errors.map((e) => ({ title: e.title, subtitle: e.sub, copy: "", bad: true }));
  const rows = bandRows(r, copy);
  const d = describe(r);
  rows.push({ title: r.mode || "Unknown command", subtitle: (from ? from + " · " : "") + d + (r.total ? ` · ${r.commandOnly ? copy.length + " bits (command only)" : r.total + " bits"}` : ""), copy: r.mode ? `${r.mode} · ${d}` : d });
  for (const e of r.errors) rows.push({ title: e.title, subtitle: e.sub, copy: "", bad: true });
  let n = 0;
  for (const f of r.frames) {
    if (f.kind === "b") continue;
    const par = f.ok ? "parity OK" : `parity NG · P should be ${f.want}`;
    const sub = f.segs.map((s) => s.bits).join(" ") + " · " + par;
    if (f.kind === "cmd") {
      const inner = r.key === "w0" ? ` · data ${hex(r.data[0], 2)}` : r.key === "w" || r.key === "r" ? ` · ${addrText(r)}` : r.bytes ? ` · BC ${r.bytes - 1} (${r.bytes} byte${r.bytes > 1 ? "s" : ""})` : "";
      rows.push({ title: `Command frame   SA ${r.sa} · ${r.mode || "unknown"}${inner}`, subtitle: sub, copy: f.bits, bad: !f.ok });
    } else if (f.kind === "addr") rows.push({ title: `Address frame   ${hex(f.value, 2)}${REG_NAMES[r.addr] && r.addr === f.value ? " " + REG_NAMES[r.addr] : ""}`, subtitle: sub, copy: hex(f.value, 2), bad: !f.ok });
    else if (f.kind === "mask") rows.push({ title: `Mask frame   ${hex(f.value, 2)}`, subtitle: sub + " · 0 = write", copy: hex(f.value, 2), bad: !f.ok });
    else {
      const many = r.data.length > 1 ? ` ${n}` : "";
      n++;
      const note = r.addr === 0x1c && r.data.length === 1 && r.mask === undefined ? ` · ${pmTrig(f.value)}` : "";
      rows.push({ title: `${isRead(r) ? "Read data" : "Data"} frame${many}   ${hex(f.value, 2)}`, subtitle: sub + note, copy: hex(f.value, 2), bad: !f.ok });
    }
  }
  return rows;
}

// 値からビット列を作る. 作れなければ理由をthrowする
function encode(key, v, mask) {
  const out = [];
  const put = (b) => out.push(b + parityOf(b));
  const sa = bin(v.sa, 4);
  const read = key === "r" || key === "er" || key === "erl";
  const data = v.data || [];
  const n = read ? v.bytes || data.length || 1 : data.length;
  if (read && data.length && data.length !== n) throw `bytes=${n} and data= have different lengths`;
  if (key === "w0") {
    if (v.addr !== undefined && v.addr !== 0) throw "Register 0 Write has no address";
    if (n !== 1 || data[0] > 0x7f) throw "data is 7 bits (up to 7F)";
    put(sa + "1" + bin(data[0], 7));
    return out.join("") + "0";
  }
  const lim = { w: 0x1f, r: 0x1f, ew: 0xff, er: 0xff, ewl: 0xffff, erl: 0xffff, mw: 0xff }[key];
  if (v.addr > lim) throw `addr is up to ${lim.toString(16).toUpperCase()}`;
  const maxN = { w: 1, r: 1, ew: 16, er: 16, ewl: 8, erl: 8, mw: 1 }[key];
  if (n < 1 || n > maxN) throw maxN === 1 ? "1 byte only" : `1 to ${maxN} bytes`;
  if (key === "w" || key === "r") put(sa + (read ? "011" : "010") + bin(v.addr, 5));
  else if (key === "ew" || key === "er") put(sa + (read ? "0010" : "0000") + bin(n - 1, 4)), put(bin(v.addr, 8));
  else if (key === "ewl" || key === "erl") put(sa + (read ? "00111" : "00110") + bin(n - 1, 3)), put(bin(v.addr >> 8, 8)), put(bin(v.addr & 0xff, 8));
  else put(sa + mask), put(bin(v.addr, 8)), put(bin(v.mask, 8));
  if (read) {
    out.push("0");
    if (!data.length) return out.join("");
  }
  data.forEach((b) => put(bin(b, 8)));
  return out.join("") + "0";
}

// `sa=6`などを読む. {vals, bad}
function parseArgs(tokens) {
  const v = {};
  for (const t of tokens) {
    const m = /^([a-z]+)=(.*)$/i.exec(t);
    if (!m || !KEYS[m[1].toLowerCase()]) return { v, bad: `${t}: use sa= addr= data= (hex)` };
    const k = m[1].toLowerCase();
    const s = m[2];
    if (s === "") continue;
    if (k === "cmd") {
      const c = CMDS.find((x) => x.key === s.toLowerCase());
      if (!c) return { v, bad: `cmd=${s}: unknown command` };
      v.cmd = c.key;
      continue;
    }
    const parts = s.split(",").filter((x) => x !== "").map((x) => x.replace(/^0x/i, ""));
    if (!parts.length || !parts.every((x) => /^[0-9a-f]+$/i.test(x))) return { v, bad: `${k}=${s}: hex only` };
    if (k === "data") {
      v.data = [];
      for (const x of parts) {
        const e = x.length % 2 ? "0" + x : x;
        for (let i = 0; i < e.length; i += 2) v.data.push(parseInt(e.substr(i, 2), 16));
      }
      continue;
    }
    if (parts.length > 1) return { v, bad: `${k}=${s}: one value` };
    const n = k === "bytes" ? parseInt(parts[0], 10) : parseInt(parts[0], 16);
    if (Number.isNaN(n)) return { v, bad: `${k}=${s}` };
    if (k === "sa" && n > 0xf) return { v, bad: "sa is 0 to F" };
    if (k === "mask" && n > 0xff) return { v, bad: "mask is up to FF" };
    if (k === "addr" && n > 0xffff) return { v, bad: "addr is up to FFFF" };
    v[k] = n;
  }
  return { v };
}

function cmdRows(prefix, typed, mask) {
  return CMDS.filter((c) => (c.key !== "mw" || mask) && c.key.startsWith(typed)).map((c) => ({
    title: `cmd=${c.key}`,
    subtitle: `${c.title} · ${c.need.map((k) => k + "=").join(" ")}`,
    fill: `${prefix}cmd=${c.key} `,
    copy: "",
  }));
}

// 値から. tokensの最後は, 打ちかけ(空白で終わっていない)かもしれない
function fromArgs(tokens, open, mask) {
  const last = open ? tokens[tokens.length - 1] : "";
  const before = "mipi " + (open ? tokens.slice(0, -1) : tokens).map((t) => t + " ").join("");
  const half = open && !last.includes("=");
  const { v, bad } = parseArgs(half ? tokens.slice(0, -1) : tokens);
  const rows = [];
  const m = /^([a-z]+)=(.*)$/i.exec(last);
  const key = m ? m[1].toLowerCase() : "";
  // cmd=の打ちかけ
  if (key === "cmd" && !CMDS.some((c) => c.key === m[2].toLowerCase())) {
    const r = cmdRows(before, m[2].toLowerCase(), mask);
    if (r.length) return r;
  }
  if (bad) return [{ title: bad, subtitle: "Example: mipi sa=6 addr=1C data=38", copy: "", bad: true }];
  const has = (k) => (k === "data" ? v.data && v.data.length : v[k] !== undefined);
  const known = Object.keys(KEYS).filter(has);
  if (v.cmd === "mw" && !mask) return [{ title: "Masked Write is not set up", subtitle: "Write its 8 command bits as masked_write in [ext.mipi] of config.toml", copy: "", bad: true }];
  // 作れるもの
  let cands = v.cmd ? CMDS.filter((c) => c.key === v.cmd) : CMDS.filter((c) => c.key !== "mw" || mask);
  if (!v.cmd) {
    cands = cands.filter((c) => {
      const read = c.need.indexOf("data") < 0;
      if (has("mask")) return c.key === "mw";
      if (c.key === "mw") return false;
      if (has("bytes")) return read;
      if (has("data")) return has("addr") ? !read && c.key !== "w0" : c.key === "w0";
      return read;
    });
  }
  const made = [];
  let why = "";
  if (!half) {
    for (const c of cands) {
      if (!c.need.every(has)) continue;
      try {
        made.push({ c, bits: encode(c.key, v, mask) });
      } catch (e) {
        if (typeof e !== "string") throw e;
        if (v.cmd) why = e;
      }
    }
  }
  if (made.length === 1) rows.push(...breakdown(decode(made[0].bits, mask), made[0].bits));
  else
    for (const x of made) {
      const r = decode(x.bits, mask);
      rows.push({ title: grouped(r), subtitle: `${x.c.title} · ${describe(r)}`, copy: x.bits, complete: `mipi cmd=${x.c.key} ${tokens.join(" ")} ` });
    }
  if (why) rows.push({ title: why, subtitle: titleOf(v.cmd), copy: "", bad: true });
  // 補完: addr=は, 名前の決まったレジスター
  if (key === "addr" && open) {
    for (const a of Object.keys(REG_NAMES).map(Number)) {
      const h = a.toString(16).toUpperCase();
      if (h.startsWith(m[2].toUpperCase()) && h !== m[2].toUpperCase()) rows.push({ title: `addr=${h}`, subtitle: REG_NAMES[a], fill: `${before}addr=${h} `, copy: "" });
    }
  }
  // 補完: まだ書いていない名前(必要なものを先に)
  const names = (prefix, typed) => {
    const want = v.cmd ? CMDS.find((c) => c.key === v.cmd).need : ["sa", "addr", "data"];
    const all = [...want, ...Object.keys(KEYS).filter((k) => want.indexOf(k) < 0)];
    for (const k of all) {
      if (known.indexOf(k) >= 0 || (k === "mask" && !mask) || !k.startsWith(typed)) continue;
      if (v.cmd && want.indexOf(k) < 0 && !(k === "bytes" && /^er/.test(v.cmd)) && !(k === "data" && /^e?rl?$/.test(v.cmd))) continue;
      rows.push({ title: `${k}=`, subtitle: KEYS[k], fill: `${prefix}${k}=`, copy: "" });
    }
  };
  if (half || !open) names(before, last.toLowerCase());
  // 値を打った直後(空白の前)でほかに出すものがなければ, 次の名前を出す. Tabで空白と名前が入る
  else if (!rows.length && m && m[2] !== "") names(`${before}${last} `, "");
  if (!rows.length && m && m[2] === "") rows.push({ title: KEYS[key] || "Type the value", subtitle: "Type the value", copy: "" });
  if (!rows.length) rows.push({ title: half ? "Use sa= addr= data= (hex), or type bits" : "Type the next value", subtitle: "Example: mipi sa=6 addr=1C data=38", copy: "" });
  return rows;
}

const onlyBits = (s) => /^[01_\s]+$/.test(s) && /[01]/.test(s);
const clean = (s) => s.replace(/[\s_]/g, "");

function readTable(text) {
  const lines = text.split(/\r?\n/).filter((l) => l.trim() !== "");
  if (lines.length < 2) return null;
  const head = lines[0].split("\t").map((s) => s.trim());
  const ci = head.findIndex((h) => h.toLowerCase() === "sclk");
  if (ci < 0) return null;
  const rows = lines.slice(1).map((l) => l.split("\t").map((s) => s.trim()));
  const cols = [];
  head.forEach((h, i) => {
    if (i === ci) return;
    const vs = rows.map((r) => (r[i] === undefined ? "" : r[i])).filter((v) => v !== "");
    if (vs.length && vs.every((v) => v === "0" || v === "1")) cols.push({ name: h || `Column ${i + 1}`, i });
  });
  return { ci, rows, cols };
}

// 表の1列. 最後のBのクロックは書かないことがあるので, 最後の1ビットだけはなくてもよい
function readColumn(t, col, mask) {
  const clk = t.rows.map((r) => r[t.ci]);
  const d = t.rows.map((r) => r[col.i]);
  if (d.every((v) => v === "0" || v === "")) return { text: "No command" };
  let ssc = -1;
  for (let i = 1; i < d.length; i++) {
    if (clk[i] === "0" && clk[i - 1] === "0" && d[i - 1] === "1" && d[i] === "0") {
      ssc = i;
      break;
    }
  }
  if (ssc < 0) return { text: "SSC not found (data falls from 1 to 0 while SCLK stays 0)" };
  let bits = "";
  for (let j = ssc; j < d.length; j++) if (clk[j] === "1") bits += d[j] === "1" ? "1" : "0";
  let r = decode(bits, mask);
  if (r.total && bits.length === r.total - 1) r = decode((bits += "0"), mask);
  else if (r.total && bits.length > r.total && /^0*$/.test(bits.slice(r.total))) r = decode((bits = bits.slice(0, r.total)), mask);
  if (r.errors.length) return { text: r.errors[0].title + " · " + r.errors[0].sub };
  return { text: `${r.mode} · ${describe(r)} · ${parityOk(r) ? "parity OK" : "parity NG"}`, copy: bits };
}

export function search(query, ctx) {
  const mask = maskCmd(ctx);
  const q = query.trim();
  if (q === "") {
    const clip = ctx.clipboard || "";
    if (onlyBits(clip) && clean(clip).length >= 13 && clean(clip).length <= 400) return breakdown(decode(clean(clip), mask), clean(clip), "From the clipboard");
    const t = readTable(clip);
    if (t) {
      if (!t.cols.length) return [{ title: "No data column (only 0 and 1) in the table", subtitle: "", copy: "" }];
      return t.cols.map((c) => {
        const r = readColumn(t, c, mask);
        return { title: c.name, subtitle: r.text, copy: r.copy || r.text };
      });
    }
    return cmdRows("mipi ", "", mask);
  }
  if (onlyBits(q)) return breakdown(decode(clean(q), mask), clean(q));
  return fromArgs(q.split(/\s+/), !/\s$/.test(ctx.raw === undefined ? query : ctx.raw), mask);
}
