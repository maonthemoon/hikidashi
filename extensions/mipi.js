// MIPI RFFEのコマンドフレームを読み解く見本の拡張(規格の読み解きだけ).
// 計算のモード(`= `)の中でも, ふだんの検索でも使える. 計算のモードでは, ビット列はそのまま打ってよい.
//   mipi 0110010111001001110000   ビット列 -> モード, SA, アドレス, データ, パリティ
//   mipi w 6 1C 38                モード, SA, アドレス, データ(16進) -> ビット列
//   mipi 6 1C 38                  モードを書かなければ, 使えるモードを全部出す
//   mipi (クリップボードにExcelの表)  SCLKの列とデータの列から読み取る
export const meta = { title: "MIPI RFFE", keyword: "mipi", clipboard: true, calc: true, calcBits: 13 };

const REG_NAMES = { 0x1c: "PM_TRIG", 0x1d: "PRODUCT_ID", 0x1e: "MANUFACTURER_ID", 0x1f: "MAN_USID" };
const PWR_MODES = ["Active", "Startup", "Low Power", "Reserved"];

const bin = (n, w) => n.toString(2).padStart(w, "0");
const hex = (n, w) => "0x" + n.toString(16).toUpperCase().padStart(w || 1, "0");
const ones = (s) => s.split("").filter((c) => c === "1").length;
// 奇数パリティ: パリティを含めた1の数が奇数になる値. コマンドフレームのパリティはSA(4ビット)を含む12ビットにかける
const parityOf = (s) => (ones(s) % 2 === 0 ? "1" : "0");

function pmTrig(d) {
  return `${PWR_MODES[d >> 6]}, mask ${bin((d >> 3) & 7, 3)}, trig ${bin(d & 7, 3)}`;
}

function regLabel(addr, aw) {
  const n = REG_NAMES[addr];
  return hex(addr, aw) + (n ? " " + n : "");
}

// ビット列を読み解く. 足りないときは{error}. 返り値のusedは使ったビット数
function decode(bits) {
  let pos = 0;
  const parities = [];
  const fields = [];
  const take = (n, name) => {
    if (pos + n > bits.length) throw "short";
    const s = bits.substr(pos, n);
    pos += n;
    if (name) fields.push({ name, bits: s });
    return s;
  };
  const par = (name, covered) => {
    const p = take(1, "P");
    parities.push({ name, ok: (ones(covered) + (p === "1" ? 1 : 0)) % 2 === 1 });
  };
  let desc;
  const info = { mode: "", sa: "", addr: "", addrBits: "", data: [] };
  try {
    const sab = take(4, "SA");
    const sa = parseInt(sab, 2);
    const head = `SA ${hex(sa)}`;
    info.sa = hex(sa);
    info.saBits = sab;
    if (bits[pos] === "1") {
      // Register 0 Write: SA, 1, データ7ビット, パリティ
      const body = take(8, "1+data[6:0]");
      par("cmd", sab + body);
      info.mode = "Register 0 Write";
      info.addr = "0x00";
      info.data = [{ v: parseInt(body.slice(1), 2), bits: body.slice(1), note: "7 bits" }];
      desc = `${head} · Register 0 Write · 0x00 ← ${hex(parseInt(body.slice(1), 2), 2)}`;
    } else if (bits.substr(pos, 3) === "010" || bits.substr(pos, 3) === "011") {
      const write = bits.substr(pos, 3) === "010";
      const cmd = take(3, "CMD");
      const abits = take(5, "ADDR");
      par("cmd", sab + cmd + abits);
      const addr = parseInt(abits, 2);
      const label = regLabel(addr, 2);
      info.mode = write ? "Register Write" : "Register Read";
      info.addr = label;
      info.addrBits = abits;
      if (write) {
        const db = take(8, "DATA");
        par("data", db);
        const d = parseInt(db, 2);
        info.data = [{ v: d, bits: db, note: addr === 0x1c ? pmTrig(d) : "" }];
        desc = `${head} · Register Write · ${label} ← ${hex(d, 2)}` + (addr === 0x1c ? ` · ${pmTrig(d)}` : "");
      } else if (bits.length - pos >= 9) {
        const db = take(8, "DATA");
        par("data", db);
        const d = parseInt(db, 2);
        info.data = [{ v: d, bits: db, note: addr === 0x1c ? pmTrig(d) : "" }];
        desc = `${head} · Register Read · ${label} → ${hex(d, 2)}` + (addr === 0x1c ? ` · ${pmTrig(d)}` : "");
      } else {
        desc = `${head} · Register Read · ${label}`;
      }
    } else if (bits.substr(pos, 4) === "0000" || bits.substr(pos, 4) === "0010") {
      const write = bits.substr(pos, 4) === "0000";
      const cmd = take(8, "CMD+BC");
      par("cmd", sab + cmd);
      const n = parseInt(cmd.slice(4), 2) + 1;
      const ab = take(8, "ADDR");
      par("addr", ab);
      const addr = parseInt(ab, 2);
      info.mode = `Extended Register ${write ? "Write" : "Read"}`;
      info.bytes = n;
      info.addr = regLabel(addr, 2);
      info.addrBits = ab;
      desc = `${head} · Extended Register ${write ? "Write" : "Read"} · ${n} byte${n > 1 ? "s" : ""} · ${regLabel(addr, 2)}`;
      desc += dataBytes(write, n, addr);
    } else if (bits.substr(pos, 5) === "00110" || bits.substr(pos, 5) === "00111") {
      const write = bits.substr(pos, 5) === "00110";
      const cmd = take(8, "CMD+BC");
      par("cmd", sab + cmd);
      const n = parseInt(cmd.slice(5), 2) + 1;
      const hi = take(8, "ADDR[15:8]");
      par("addr hi", hi);
      const lo = take(8, "ADDR[7:0]");
      par("addr lo", lo);
      const addr = parseInt(hi + lo, 2);
      info.mode = `Extended Register ${write ? "Write" : "Read"} Long`;
      info.bytes = n;
      info.addr = hex(addr, 4);
      info.addrBits = hi + " " + lo;
      desc = `${head} · Extended Register ${write ? "Write" : "Read"} Long · ${n} byte${n > 1 ? "s" : ""} · ${hex(addr, 4)}`;
      desc += dataBytes(write, n, addr);
    } else {
      return { error: `unknown command (${bits.substr(pos, 8)})`, desc: `${head} · unknown command`, fields, parities, used: pos, info };
    }
  } catch (e) {
    if (e !== "short") throw e;
    return { error: `Not enough bits (${bits.length})`, desc, fields, parities, used: pos, info };
  }
  return { desc, fields, parities, used: pos, info };

  function dataBytes(write, n, addr) {
    const vals = [];
    let more = "";
    for (let i = 0; i < n; i++) {
      if (!write && bits.length - pos < 9) break; // Readはデータがなくてもよい
      const db = take(8, "DATA");
      par("data" + (n > 1 ? i : ""), db);
      vals.push(parseInt(db, 2));
      info.data.push({ v: parseInt(db, 2), bits: db, note: addr === 0x1c && n === 1 ? pmTrig(parseInt(db, 2)) : "" });
    }
    if (vals.length) {
      more = ` ${write ? "←" : "→"} ${vals.map((v) => hex(v, 2)).join(" ")}`;
      if (vals.length === 1 && n === 1 && addr === 0x1c) more += ` · ${pmTrig(vals[0])}`;
    }
    return more;
  }
}

function parityText(r) {
  return r.parities.every((p) => p.ok) ? "parity OK" : "parity NG";
}

function parseHex(t) {
  const s = t.replace(/^0x/i, "");
  return /^[0-9a-f]+$/i.test(s) ? { v: parseInt(s, 16), digits: s.length, text: s } : null;
}

// コマンドの種類. 打つ名前, 名前, 引数の形, 例
const MODES = [
  { key: "w", names: ["w", "write"], title: "Register Write", args: "SA ADDRESS DATA", ex: "w 6 1C 38" },
  { key: "r", names: ["r", "read"], title: "Register Read", args: "SA ADDRESS", ex: "r 6 1C" },
  { key: "ew", names: ["ew", "extwrite"], title: "Extended Register Write", args: "SA ADDRESS DATA… (1 to 16 bytes)", ex: "ew 6 40 12 34" },
  { key: "er", names: ["er", "extread"], title: "Extended Register Read", args: "SA ADDRESS BYTES", ex: "er 6 40 2" },
  { key: "ewl", names: ["ewl", "extwritelong"], title: "Extended Register Write Long", args: "SA ADDRESS(16 bits) DATA… (1 to 8 bytes)", ex: "ewl 6 1234 AB" },
  { key: "erl", names: ["erl", "extreadlong"], title: "Extended Register Read Long", args: "SA ADDRESS(16 bits) BYTES", ex: "erl 6 1234 2" },
  { key: "w0", names: ["w0", "reg0"], title: "Register 0 Write", args: "SA DATA (7 bits)", ex: "w0 6 38" },
];
const BITS_EX = "0110010111001001110000";

// 値(16進)からフレームを作る. グループ(SAなど)の配列を返す. 作れなければ理由をthrowする
function encode(key, sa, rest) {
  const g = [bin(sa, 4)];
  const cmd = (b) => g.push(b, parityOf(g[0] + b));
  const put = (b) => g.push(b, parityOf(b));
  if (key === "w0") {
    if (rest.length !== 1 || rest[0].v > 0x7f) throw "Register 0 Write: DATA is up to 0x7F";
    cmd("1" + bin(rest[0].v, 7));
    return g;
  }
  if (!rest.length) throw "Type the address";
  const addr = rest[0].v;
  const read = key[0] === "r" || key[1] === "r";
  let bytes = [];
  if (read) {
    bytes = new Array(rest[1] ? rest[1].v : 1).fill(0);
  } else if (rest.length === 2 && rest[1].digits > 2) {
    const p = rest[1].text.length % 2 ? "0" + rest[1].text : rest[1].text;
    for (let i = 0; i < p.length; i += 2) bytes.push(parseInt(p.substr(i, 2), 16));
  } else {
    bytes = rest.slice(1).map((v) => v.v);
    if (bytes.some((b) => b > 0xff)) throw "DATA: one byte each (up to 0xFF)";
  }
  if (!bytes.length) throw read ? "BYTES is 1 or more" : "Type the data";
  const n = bytes.length;
  if (key === "w" || key === "r") {
    if (addr > 0x1f) throw "ADDRESS is up to 0x1F (use ew or er)";
    if (n !== 1) throw "1 byte only (use ew or er)";
    const c = key === "w" ? "010" : "011";
    g.push(c, bin(addr, 5), parityOf(g[0] + c + bin(addr, 5)));
  } else if (key === "ew" || key === "er") {
    if (addr > 0xff) throw "ADDRESS is up to 0xFF (use ewl or erl)";
    if (n > 16) throw "Up to 16 bytes";
    cmd((key === "ew" ? "0000" : "0010") + bin(n - 1, 4));
    put(bin(addr, 8));
  } else {
    if (addr > 0xffff) throw "ADDRESS is up to 0xFFFF";
    if (n > 8) throw "Up to 8 bytes";
    cmd((key === "ewl" ? "00110" : "00111") + bin(n - 1, 3));
    put(bin(addr >> 8, 8));
    put(bin(addr & 0xff, 8));
  }
  if (!read) for (const b of bytes) put(bin(b, 8));
  return g;
}

function frameRow(groups) {
  const bits = groups.join("");
  const r = decode(bits);
  return { title: groups.join(" "), subtitle: r.error ? r.error : r.desc, copy: bits };
}

function fromValues(tokens) {
  const first = tokens[0].toLowerCase();
  const mode = MODES.find((m) => m.names.includes(first));
  const vals = (mode ? tokens.slice(1) : tokens).map(parseHex);
  const usage = mode ? { title: `${mode.key} ${mode.args}`, subtitle: `${mode.title} · Example: ${mode.ex} (hex)`, fill: "mipi " + mode.ex } : null;
  if (mode && !vals.length) return [usage];
  if (vals.some((v) => !v)) return [{ title: "Type hex values", subtitle: "Example: mipi w 6 1C 38", fill: "mipi w 6 1C 38" }];
  const sa = vals[0].v;
  if (sa > 0xf) return [{ title: "SA is 0x0 to 0xF", subtitle: "", copy: "" }];
  const rest = vals.slice(1);
  if (mode) {
    try {
      return [frameRow(encode(mode.key, sa, rest))];
    } catch (e) {
      if (typeof e !== "string") throw e;
      return [{ title: e, subtitle: "", copy: "" }, usage];
    }
  }
  // モードを書いていない: 使えるものを全部出す
  const keys = rest.length === 0 ? [] : rest.length === 1 ? ["r", "w0", "er", "erl"] : ["w", "ew", "ewl"];
  const rows = [];
  for (const k of keys) {
    try {
      rows.push(frameRow(encode(k, sa, rest)));
    } catch (e) {
      if (typeof e !== "string") throw e;
    }
  }
  return rows.length ? rows : usageRows();
}

function usageRows() {
  return [{ title: BITS_EX, subtitle: "Bits to mode, SA, address, data, parity", fill: "mipi " + BITS_EX }].concat(
    MODES.map((m) => ({ title: m.ex, subtitle: `${m.title} to bits · ${m.key} ${m.args}`, fill: "mipi " + m.ex }))
  );
}

function fromBits(raw) {
  const bits = raw.replace(/[\s_]/g, "");
  const r = decode(bits);
  const extra = bits.length - r.used;
  // 最後のBP(0)は1つだけ付いていてもよい
  const note = extra === 0 || (extra === 1 && bits[bits.length - 1] === "0") ? "" : ` · ${extra} extra bits`;
  const i = r.info;
  const frame = { title: r.fields.map((f) => f.bits).join(" ") || bits, subtitle: "Frame · " + r.fields.map((f) => f.name).join(" "), copy: bits.slice(0, r.used) };
  const head = (r.error ? r.error + (r.desc ? " · " + r.desc : "") : r.desc + " · " + parityText(r)) + note;
  const rows = [{ title: i.mode || "Unknown command", subtitle: head, copy: head }];
  if (i.sa) rows.push({ title: `SA ${i.sa}`, subtitle: `Slave address · ${i.saBits}`, copy: i.sa });
  if (i.addr) rows.push({ title: `Address ${i.addr}`, subtitle: i.addrBits || "Register 0", copy: i.addr.split(" ")[0] });
  i.data.forEach((d, n) => rows.push({ title: `Data${i.data.length > 1 ? " " + n : ""} ${hex(d.v, 2)}`, subtitle: d.bits + (d.note ? " · " + d.note : ""), copy: hex(d.v, 2) }));
  if (!i.data.length && i.bytes) rows.push({ title: `${i.bytes} byte${i.bytes > 1 ? "s" : ""}`, subtitle: "Read length", copy: String(i.bytes) });
  if (r.parities.length) rows.push({ title: r.parities.every((p) => p.ok) ? "Parity OK" : "Parity NG", subtitle: "Odd parity · " + r.parities.map((p) => `${p.name} ${p.ok ? "OK" : "NG"}`).join(" · "), copy: r.parities.every((p) => p.ok) ? "OK" : "NG" });
  rows.push(frame);
  return rows;
}

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

function readColumn(t, col) {
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
  const r = decode(bits);
  if (r.error) return { text: r.error };
  return { text: r.desc + " · " + parityText(r), copy: bits.slice(0, r.used) };
}

export function search(query, ctx) {
  const q = query.trim();
  if (q === "") {
    const t = readTable(ctx.clipboard || "");
    if (!t) return usageRows();
    if (!t.cols.length) return [{ title: "No data column (only 0 and 1) in the table", subtitle: "", copy: "" }];
    return t.cols.map((c) => {
      const r = readColumn(t, c);
      return { title: c.name, subtitle: r.text, copy: r.copy || r.text };
    });
  }
  const tokens = q.split(/\s+/);
  const onlyBits = tokens.every((s) => /^[01_]+$/.test(s));
  if (onlyBits && tokens.join("").replace(/_/g, "").length >= 12) return fromBits(q);
  return fromValues(tokens);
}
