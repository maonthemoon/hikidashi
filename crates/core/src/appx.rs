//! パッケージのアプリ(ストア版のOfficeなど)のマニフェスト`AppxManifest.xml`から, exeの名前を取る.
//! XMLの部品は使わず, `<Application`の開きタグの属性だけを読む.

/// `Id`が`id`の`<Application>`の`Executable`の, ファイル名の拡張子を除いた部分(`Office16\WINWORD.EXE`→`WINWORD`).
pub fn manifest_exe(xml: &str, id: &str) -> Option<String> {
    for part in xml.split("<Application").skip(1) {
        // `<ApplicationContentUriRules>`などの別のタグは除く
        if !part.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        let tag = &part[..part.find('>')?];
        if attr(tag, "Id").is_some_and(|v| v.eq_ignore_ascii_case(id)) {
            let exe = attr(tag, "Executable")?;
            let file = exe.rsplit(['\\', '/']).next()?;
            let stem = file.rsplit_once('.').map(|(a, _)| a).unwrap_or(file);
            return (!stem.is_empty()).then(|| stem.to_string());
        }
    }
    None
}

/// タグの中の`name="値"`. 前が空白のものだけ(`AppId`を`Id`と取り違えない).
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let pat = format!("{name}=\"");
    let mut from = 0;
    while let Some(i) = tag[from..].find(&pat) {
        let at = from + i;
        if tag[..at].ends_with(|c: char| c.is_whitespace()) {
            let rest = &tag[at + pat.len()..];
            return Some(&rest[..rest.find('"')?]);
        }
        from = at + pat.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<Package>
  <Applications>
    <Application Id="Excel"
      Executable="Office16\EXCEL.EXE" EntryPoint="Windows.FullTrustApplication">
    </Application>
    <Application	Id="Word" Executable="Office16\WINWORD.EXE">
      <uap:ApplicationContentUriRules/>
    </Application>
    <Application AppId="x" Id="Calc" Executable="calc.exe"/>
  </Applications>
</Package>"#;

    #[test]
    fn finds_exe_by_id() {
        assert_eq!(manifest_exe(XML, "Word").as_deref(), Some("WINWORD"));
        assert_eq!(manifest_exe(XML, "excel").as_deref(), Some("EXCEL"));
        assert_eq!(manifest_exe(XML, "Calc").as_deref(), Some("calc"));
        assert_eq!(manifest_exe(XML, "PowerPoint"), None);
    }
}
