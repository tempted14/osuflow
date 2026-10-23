//! Recognising osu! beatmap links among the URLs Windows hands to a browser.
//!
//! osu! stable opens `https://osu.ppy.sh/b/{id}` when a multiplayer map is missing, chat links
//! and the website use the other forms. Only plain beatmap pages count: a discussion page or a
//! user profile is something the person wants to read, so it goes to their real browser.

use reqwest::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkTarget {
    /// A beatmapset id, which is what mirrors download.
    Set(u32),
    /// A single beatmap (difficulty) id, which has to be looked up to find its set.
    Beatmap(u32),
}

const BEATMAP_HOSTS: [&str; 2] = ["osu.ppy.sh", "old.ppy.sh"];

/// An http(s) URL with a host. Nothing else is ever opened or forwarded.
pub fn is_web_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
}

fn id(segment: &str) -> Option<u32> {
    if segment.is_empty() || !segment.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    segment.parse().ok().filter(|id| *id > 0)
}

/// `/beatmapsets/{id}` and `/s/{id}` are sets; `/beatmaps/{id}` and `/b/{id}` are single
/// beatmaps. A `#osu/456` fragment or `?m=0` query is ignored; extra path parts (a discussion
/// page, say) make it not a plain beatmap link.
pub fn parse_link(url: &Url) -> Option<LinkTarget> {
    if !is_web_url(url) || !BEATMAP_HOSTS.contains(&url.host_str()?) {
        return None;
    }
    let mut segments = url.path_segments()?.filter(|s| !s.is_empty());
    let kind = segments.next()?;
    let value = segments.next()?;
    if segments.next().is_some() {
        return None;
    }
    match kind {
        "beatmapsets" | "s" => id(value).map(LinkTarget::Set),
        "beatmaps" | "b" => id(value).map(LinkTarget::Beatmap),
        _ => None,
    }
}

/// The first http(s) URL among the command-line arguments Windows passes when it opens a link.
pub fn first_link_in_args<I: IntoIterator<Item = String>>(args: I) -> Option<Url> {
    args.into_iter().find_map(|arg| Url::parse(&arg).ok().filter(is_web_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Option<LinkTarget> {
        parse_link(&Url::parse(text).unwrap())
    }

    #[test]
    fn recognises_every_beatmap_page_form() {
        assert_eq!(parse("https://osu.ppy.sh/beatmapsets/39804"), Some(LinkTarget::Set(39804)));
        assert_eq!(parse("https://osu.ppy.sh/beatmapsets/39804#osu/129891"), Some(LinkTarget::Set(39804)));
        assert_eq!(parse("https://osu.ppy.sh/beatmapsets/39804/"), Some(LinkTarget::Set(39804)));
        assert_eq!(parse("https://osu.ppy.sh/s/39804"), Some(LinkTarget::Set(39804)));
        assert_eq!(parse("https://osu.ppy.sh/b/129891"), Some(LinkTarget::Beatmap(129891)));
        assert_eq!(parse("https://osu.ppy.sh/b/129891?m=0"), Some(LinkTarget::Beatmap(129891)));
        assert_eq!(parse("https://osu.ppy.sh/beatmaps/129891"), Some(LinkTarget::Beatmap(129891)));
        assert_eq!(parse("http://old.ppy.sh/b/129891"), Some(LinkTarget::Beatmap(129891)));
        assert_eq!(parse("https://OSU.PPY.SH/b/129891"), Some(LinkTarget::Beatmap(129891)));
    }

    #[test]
    fn pages_people_want_to_read_are_not_beatmap_links() {
        for url in [
            "https://osu.ppy.sh/beatmapsets/39804/discussion",
            "https://osu.ppy.sh/beatmapsets/39804/discussion/-/generalAll",
            "https://osu.ppy.sh/users/2",
            "https://osu.ppy.sh/beatmapsets",
            "https://osu.ppy.sh/",
            "https://osu.ppy.sh/wiki/en/Beatmap",
            "https://osu.ppy.sh/b/129891/extra",
        ] {
            assert_eq!(parse(url), None, "{url}");
        }
    }

    #[test]
    fn other_sites_and_bad_ids_are_not_beatmap_links() {
        for url in [
            "https://example.com/b/129891",
            "https://osu.ppy.sh.evil.example/b/129891",
            "https://evil.example/osu.ppy.sh/b/1",
            "https://osu.ppy.sh/b/abc",
            "https://osu.ppy.sh/b/0",
            "https://osu.ppy.sh/b/-5",
            "https://osu.ppy.sh/b/12x",
            "https://osu.ppy.sh/b/99999999999",
            "https://osu.ppy.sh/b/",
        ] {
            assert_eq!(parse(url), None, "{url}");
        }
    }

    #[test]
    fn only_http_and_https_are_web_urls() {
        assert!(is_web_url(&Url::parse("https://example.com/x").unwrap()));
        assert!(is_web_url(&Url::parse("http://example.com").unwrap()));
        for url in ["file:///C:/Windows/notepad.exe", "ftp://example.com/x", "javascript:alert(1)", "osu://b/129891"] {
            assert!(!is_web_url(&Url::parse(url).unwrap()), "{url}");
        }
        assert_eq!(parse("ftp://osu.ppy.sh/b/129891"), None);
    }

    #[test]
    fn finds_the_link_among_command_line_arguments() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            first_link_in_args(args(&["https://osu.ppy.sh/b/1"])).map(|u| u.to_string()),
            Some("https://osu.ppy.sh/b/1".to_string())
        );
        assert_eq!(
            first_link_in_args(args(&["--flag", "C:\\notes.txt", "https://example.com/a", "https://osu.ppy.sh/b/1"]))
                .map(|u| u.to_string()),
            Some("https://example.com/a".to_string())
        );
        assert_eq!(first_link_in_args(args(&[])), None);
        assert_eq!(first_link_in_args(args(&["--silent", "file:///C:/x", "C:\\path\\x.exe"])), None);
    }
}
