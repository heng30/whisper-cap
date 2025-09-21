use font_kit::source::SystemSource;
use std::collections::HashSet;

fn is_chinese_font(font_name: &str) -> bool {
    // 检查字体名称是否包含中文Unicode字符
    for c in font_name.chars() {
        // 基本CJK汉字 CJK扩展A CJK扩展B
        if ('\u{4E00}' <= c && c <= '\u{9FFF}')
            || ('\u{3400}' <= c && c <= '\u{4DBF}')
            || ('\u{20000}' <= c && c <= '\u{2A6DF}')
        {
            return true;
        }
    }

    let chinese_keywords = [
        "宋体",
        "黑体",
        "楷体",
        "仿宋",
        "雅黑",
        "Song",
        "Hei",
        "Fang",
        "YaHei",
        "PingFang",
        "Hiragino",
        "WenQuanYi",
        "Source Han",
        "Microsoft",
        "华文",
        "思源",
    ];

    for keyword in chinese_keywords {
        if font_name.contains(keyword) {
            return true;
        }
    }

    false
}

pub fn system_fonts() -> (Vec<(String, String)>, Vec<(String, String)>) {
    let source = SystemSource::new();
    let mut chinese_fonts = HashSet::new();
    let mut none_chinese_fonts = HashSet::new();

    if let Ok(families) = source.all_families() {
        for family_name in families {
            if let Ok(family_handle) = source.select_family_by_name(&family_name) {
                let fonts = family_handle.fonts();
                for font_handle in fonts {
                    if let Ok(font) = font_handle.load() {
                        let font_name = font.full_name();
                        let family = font.family_name();

                        let font_info = (font_name, family);

                        if is_chinese_font(&font_info.0) {
                            chinese_fonts.insert(font_info);
                        } else {
                            none_chinese_fonts.insert(font_info);
                        }
                    }
                }
            }
        }
    }

    let chinese_fonts = Vec::from_iter(chinese_fonts.into_iter());
    let none_chinese_fonts = Vec::from_iter(none_chinese_fonts.into_iter());

    (chinese_fonts, none_chinese_fonts)
}
