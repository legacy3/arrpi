//! Native Watcher destinations; authentication remains owned by `CrossWatch`.

pub(super) struct Destination {
    pub(super) id: &'static str,
    pub(super) label: &'static str,
    pub(super) credential: &'static str,
    pub(super) disconnect: &'static str,
}

macro_rules! destinations {
    ($(($id:literal, $label:literal, $credential:literal, $disconnect:literal)),+ $(,)?) => {
        pub(super) const DESTINATIONS: &[Destination] = &[
            $(Destination { id: $id, label: $label, credential: $credential, disconnect: $disconnect }),+
        ];
    };
}

destinations! {
    ("trakt", "Trakt", "access_token", "token/delete"),
    ("simkl", "SIMKL", "access_token", "token/delete"),
    ("anilist", "AniList", "access_token", "token/delete"),
    ("myanimelist", "MyAnimeList", "access_token", "disconnect"),
    ("kitsu", "Kitsu", "access_token", "disconnect"),
    ("mdblist", "MDBList", "access_token", "disconnect"),
    ("bingebase", "BingeBase", "webhook_url", "disconnect"),
    ("flicklist", "FlickList", "access_token", "disconnect"),
    ("punchplay", "PunchPlay", "access_token", "disconnect"),
    ("wetrakr", "WeTrakr", "access_token", "disconnect"),
    ("crosswatch", "CrossWatch Local Tracker", "connected", "disconnect"),
    ("floppy", "Floppy", "api_token", "disconnect"),
    ("scrob", "Scrob", "api_key", "disconnect"),
    ("jellyfin", "Jellyfin", "access_token", "token/delete"),
    ("emby", "Emby", "access_token", "token/delete"),
    ("kodi", "Kodi", "connection_verified", "disconnect"),
}

pub(super) fn destination(id: &str) -> Option<&'static Destination> {
    DESTINATIONS.iter().find(|provider| provider.id == id)
}

pub(super) fn pin_flow(id: &str) -> Option<(&'static str, &'static str)> {
    match id {
        "simkl" => Some(("pin", concat!("https://", "simkl.com", "/pin"))),

        "mdblist" => Some((
            "device",
            concat!("https://", "mdblist.com", "/oauth/device/"),
        )),

        "punchplay" => Some(("device", concat!("https://", "punchplay.tv", "/link"))),

        "bingebase" => Some(("device", concat!("https://", "bingebase.com", "/activate"))),

        "flicklist" => Some(("device", concat!("https://", "flicklist.tv", "/link"))),

        _ => None,
    }
}
