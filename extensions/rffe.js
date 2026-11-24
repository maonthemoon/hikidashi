// MIPI RFFEのコマンドフレームを読み解く見本の拡張(規格の読み解きだけ).
// 計算のモード(`= `)の中でも, ふだんの検索でも使える.
//   rffe 6 1C 38          SA, レジスターのアドレス, データ(16進) -> フレームのビット列
//   rffe 0110010111001001110000   ビット列 -> SA, コマンド, アドレス, データ, パリティ
//   rffe (クリップボードにExcelの表)  SCLKの列とデータの列から読み取る
export const meta = { title: "MIPI RFFE", keyword: "rffe", clipboard: true, calc: true };

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
  try {
    const sab = take(4, "SA");
    const sa = parseInt(sab, 2);
    const head = `SA ${hex(sa)}`;
    if (bits[pos] === "1") {
      // Register 0 Write: SA, 1, データ7ビット, パリティ
      const body = take(8, "1+data[6:0]");
      par("cmd", sab + body);
      desc = `${head} · Register 0 Write · 0x00 ← ${hex(parseInt(body.slice(1), 2), 2)}`;
    } else if (bits.substr(pos, 3) === "010" || bits.substr(pos, 3) === "011") {
      const write = bits.substr(pos, 3) === "010";
      const cmd = take(3, "CMD");
      const abits = take(5, "ADDR");
      par("cmd", sab + cmd + abits);
      const addr = parseInt(abits, 2);
      const label = regLabel(addr, 2);
      if (write) {
        const db = take(8, "DATA");
        par("data", db);
        const d = parseInt(db, 2);
        desc = `${head} · Register Write · ${label} ← ${hex(d, 2)}` + (addr === 0x1c ? ` · ${pmTrig(d)}` : "");
      } else if (bits.length - pos >= 9) {
        const db = take(8, "DATA");
        par("data", db);
        const d = parseInt(db, 2);
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
      desc = `${head} · Extended Register ${write ? "Write" : "Read"} Long · ${n} byte${n > 1 ? "s" : ""} · ${hex(addr, 4)}`;
      desc += dataBytes(write, n, addr);
    } else {
      return { error: `unknown command (${bits.substr(pos, 8)})`, desc: `${head} · unknown command`, fields, parities, used: pos };
    }
  } catch (e) {
    if (e !== "short") throw e;
    return { error: `ビットが足りない(${bits.length}ビット)`, fields, parities, used: pos };
  }
  return { desc, fields, parities, used: pos };

  function dataBytes(write, n, addr) {
    const vals = [];
    let more = "";
    for (let i = 0; i < n; i++) {
      if (!write && bits.length - pos < 9) break; // Readはデータがなくてもよい
      const db = take(8, "DATA");
      par("data" + (n > 1 ? i : ""), db);
      vals.push(parseInt(db, 2));
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
  return /^[0-9a-f]+$/i.test(s) ? { v: parseInt(s, 16), digits: s.length } : null;
}

// 値からフレームを作る. グループ(SAなど)の配列を返す
function encode(tokens) {
  const vals = tokens.map(parseHex);
  if (vals.some((v) => !v)) throw "16進で入力してください";
  const sa = vals[0].v;
  if (sa > 0xf) throw "SAは0x0-0xF";
  const g = [bin(sa, 4)];
  const put = (bitsStr) => {
    g.push(bitsStr, parityOf(bitsStr));
  };
  if (vals.length === 2) {
    if (vals[1].v > 0x7f) throw "Register 0 Writeのデータは7ビットまで(0x7F以下)";
    g.push("1" + bin(vals[1].v, 7), parityOf(g[0] + "1" + bin(vals[1].v, 7)));
    return g;
  }
  const addr = vals[1].v;
  let bytes = [];
  if (vals.length === 3 && vals[2].digits > 2) {
    const h = tokens[2].replace(/^0x/i, "");
    const p = h.length % 2 ? "0" + h : h;
    for (let i = 0; i < p.length; i += 2) bytes.push(parseInt(p.substr(i, 2), 16));
  } else {
    bytes = vals.slice(2).map((v) => v.v);
    if (bytes.some((b) => b > 0xff)) throw "データは1バイトずつ(0xFF以下)で書く";
  }
  if (vals.length === 3 && bytes.length === 1 && addr <= 0x1f) {
    g.push("010", bin(addr, 5), parityOf(g[0] + "010" + bin(addr, 5)), bin(bytes[0], 8), parityOf(bin(bytes[0], 8)));
    return g;
  }
  if (addr > 0xffff) throw "アドレスは0xFFFFまで";
  const n = bytes.length;
  if (addr <= 0xff) {
    if (n > 16) throw "Extended Register Writeは16バイトまで";
    g.push("0000" + bin(n - 1, 4), parityOf(g[0] + "0000" + bin(n - 1, 4)));
    put(bin(addr, 8));
  } else {
    if (n > 8) throw "Extended Register Write Longは8バイトまで";
    g.push("00110" + bin(n - 1, 3), parityOf(g[0] + "00110" + bin(n - 1, 3)));
    put(bin(addr >> 8, 8));
    put(bin(addr & 0xff, 8));
  }
  for (const b of bytes) put(bin(b, 8));
  return g;
}

function groupedRow(groups, extra) {
  const bits = groups.join("");
  const r = decode(bits);
  return {
    title: groups.join(" "),
    subtitle: r.error ? r.error : r.desc + (extra === false ? "" : " · " + parityText(r)),
    copy: bits,
  };
}

function fromBits(raw) {
  let bits = raw.replace(/[\s_]/g, "");
  const r0 = decode(bits);
  const extra = bits.length - r0.used;
  const rows = [];
  // 最後のBP(0)は1つだけ付いていてもよい
  const note = extra === 0 || (extra === 1 && bits[bits.length - 1] === "0") ? "" : ` · 余りが${extra}ビット`;
  const names = r0.fields.map((f) => f.bits);
  rows.push({
    title: names.join(" ") || bits,
    subtitle: (r0.error ? r0.error + (r0.desc ? " · " + r0.desc : "") : r0.desc + " · " + parityText(r0)) + note,
    copy: bits.slice(0, r0.used),
  });
  for (const f of r0.fields) {
    rows.push({ title: `${f.name}: ${f.bits}`, subtitle: "", copy: f.bits });
  }
  // パリティごとの結果
  for (const p of r0.parities) {
    rows.push({ title: `parity(${p.name}) ${p.ok ? "OK" : "NG"}`, subtitle: "奇数パリティ", copy: p.ok ? "OK" : "NG" });
  }
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
    if (vs.length && vs.every((v) => v === "0" || v === "1")) cols.push({ name: h || `列${i + 1}`, i });
  });
  return { ci, rows, cols };
}

function readColumn(t, col) {
  const clk = t.rows.map((r) => r[t.ci]);
  const d = t.rows.map((r) => r[col.i]);
  if (d.every((v) => v === "0" || v === "")) return { text: "コマンドなし" };
  let ssc = -1;
  for (let i = 1; i < d.length; i++) {
    if (clk[i] === "0" && clk[i - 1] === "0" && d[i - 1] === "1" && d[i] === "0") {
      ssc = i;
      break;
    }
  }
  if (ssc < 0) return { text: "SSCが見つからない(SCLKが0のままデータが1から0になる所がない)" };
  let bits = "";
  for (let j = ssc; j < d.length; j++) if (clk[j] === "1") bits += d[j] === "1" ? "1" : "0";
  const r = decode(bits);
  if (r.error) return { text: r.error };
  return { text: r.desc + " · " + parityText(r), copy: bits.slice(0, r.used) };
}

export function search(query, ctx) {
  const q = query.trim();
  const usage = {
    title: "rffe 6 1C 38 (SA アドレス データ) / ビット列 / 表をコピーしてrffe",
    subtitle: "MIPI RFFEのフレームを読み解く. 値は16進",
    copy: "rffe 6 1C 38",
  };
  if (q === "") {
    const t = readTable(ctx.clipboard || "");
    if (!t) return [usage];
    if (!t.cols.length) return [{ title: "表にデータの列(0と1だけの列)がない", subtitle: "", copy: "" }];
    return t.cols.map((c) => {
      const r = readColumn(t, c);
      return { title: c.name, subtitle: r.text, copy: r.copy || r.text };
    });
  }
  const tokens = q.split(/\s+/);
  const onlyBits = tokens.every((s) => /^[01_]+$/.test(s));
  if (onlyBits && tokens.join("").replace(/_/g, "").length >= 12) return fromBits(q);
  try {
    if (tokens.length < 2) throw "rffe 6 1C 38 のようにSA アドレス データを入力";
    return [groupedRow(encode(tokens), false)];
  } catch (e) {
    if (typeof e !== "string") throw e;
    return [{ title: e, subtitle: "", copy: "" }];
  }
}
