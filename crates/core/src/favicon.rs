//! ブックマークのアイコン(favicon. 2026-11-24). ChromeとEdgeのプロフィールの`Favicons`(SQLite)から, ページのURLで引く.
//! ブラウザーが使っている間は鍵がかかるので, 画面の側で一時フォルダーに写してから開く.
//! 表: `icon_mapping`(page_url → icon_id), `favicon_bitmaps`(icon_id, width, image_data = PNG).

use rusqlite::Connection;
use std::collections::HashMap;

/// URLごとのアイコン(PNG). `want`は描く大きさ(ピクセル). それ以上で一番小さいもの, なければ一番大きいものを選ぶ
pub fn read(conn: &Connection, urls: &[String], want: i64) -> HashMap<String, Vec<u8>> {
    let mut out = HashMap::new();
    let Ok(mut stmt) = conn.prepare(
        "SELECT b.image_data FROM icon_mapping m JOIN favicon_bitmaps b ON b.icon_id = m.icon_id \
         WHERE m.page_url = ?1 AND length(b.image_data) > 0 \
         ORDER BY (b.width >= ?2) DESC, CASE WHEN b.width >= ?2 THEN b.width ELSE -b.width END ASC LIMIT 1",
    ) else {
        return out;
    };
    for url in urls {
        if out.contains_key(url) {
            continue;
        }
        // 末尾の`/`のあるなしの違いも見る
        let alt = match url.strip_suffix('/') {
            Some(u) => u.to_string(),
            None => format!("{url}/"),
        };
        for u in [url.as_str(), alt.as_str()] {
            if let Ok(png) = stmt.query_row(rusqlite::params![u, want], |r| r.get::<_, Vec<u8>>(0)) {
                out.insert(url.clone(), png);
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_size_and_slash() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE icon_mapping(id INTEGER PRIMARY KEY, page_url TEXT, icon_id INTEGER);
             CREATE TABLE favicon_bitmaps(id INTEGER PRIMARY KEY, icon_id INTEGER, width INTEGER, image_data BLOB);
             INSERT INTO icon_mapping(page_url, icon_id) VALUES ('https://a.example/', 1), ('https://b.example/x', 2);
             INSERT INTO favicon_bitmaps(icon_id, width, image_data) VALUES (1, 16, x'10'), (1, 32, x'20'), (1, 64, x'40'), (2, 16, x'16');",
        )
        .unwrap();
        let urls = vec!["https://a.example".to_string(), "https://b.example/x".to_string(), "https://none.example/".to_string()];
        let m = read(&c, &urls, 28);
        assert_eq!(m.get("https://a.example"), Some(&vec![0x20]));
        // 小さいものしかなければそれを使う
        assert_eq!(m.get("https://b.example/x"), Some(&vec![0x16]));
        assert!(!m.contains_key("https://none.example/"));
        // 表がなければ空
        let empty = Connection::open_in_memory().unwrap();
        assert!(read(&empty, &urls, 28).is_empty());
    }
}
