use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
use url::Url;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Session {
    pub access_token: String,
    pub user: User,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct User {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Default, Debug, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub overview: String,
    #[serde(rename = "Type")]
    pub kind: String,
    pub production_year: Option<u32>,
    pub community_rating: Option<f32>,
    pub run_time_ticks: Option<u64>,
    pub user_data: UserData,
    pub series_name: Option<String>,
    pub index_number: Option<u32>,
    pub parent_index_number: Option<u32>,
    pub index_number_end: Option<u32>,
    pub series_id: Option<String>,
    pub season_id: Option<String>,
    pub child_count: Option<u32>,
    pub genres: Vec<String>,
    pub people: Vec<Person>,
    pub image_tags: std::collections::HashMap<String, String>,
}
#[derive(Clone, Default, Debug, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub role: Option<String>,
    #[serde(rename = "Type")]
    pub kind: String,
    pub primary_image_tag: Option<String>,
}
#[derive(Clone, Default, Debug, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct UserData {
    pub playback_position_ticks: u64,
    pub played_percentage: Option<f32>,
    pub is_favorite: bool,
    pub played: bool,
    pub unplayed_item_count: Option<u32>,
}
impl Item {
    pub fn season_label(&self) -> String {
        match self.index_number {
            Some(0) => "Specials".into(),
            Some(n) => format!("Season {n}"),
            None => self.name.clone(),
        }
    }
    pub fn episode_code(&self) -> String {
        match (self.parent_index_number, self.index_number) {
            (Some(s), Some(e)) => format!("S{s:02}E{e:02}"),
            (_, Some(e)) => format!("Episode {e}"),
            _ => "Episode".into(),
        }
    }
    pub fn playback_title(&self) -> String {
        if self.kind == "Episode" {
            format!(
                "{} · {} · {}",
                self.series_name.as_deref().unwrap_or("Series"),
                self.episode_code(),
                self.name
            )
        } else {
            self.name.clone()
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryPage {
    pub items: Vec<Item>,
    #[serde(default)]
    pub total_record_count: usize,
}
pub struct ExternalSubtitle {
    pub url: String,
    pub title: String,
    pub language: String,
}
pub struct PlaybackPlan {
    pub url: String,
    pub subtitles: Vec<ExternalSubtitle>,
}
#[derive(Clone)]
pub struct Api {
    pub base: Url,
    client: Client,
    auth: String,
    pub session: Option<Session>,
}
impl Api {
    pub fn new(server: &str, device: &str) -> Result<Self, String> {
        let base = normalize_server(server)?;
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            base,
            client,
            auth: format!(
                "MediaBrowser Client=\"Aster\", Device=\"Living Room\", DeviceId=\"{device}\", Version=\"{}\"",
                env!("CARGO_PKG_VERSION")
            ),
            session: None,
        })
    }
    fn url(&self, path: &str) -> Url {
        self.base
            .join(path.trim_start_matches('/'))
            .expect("static API path")
    }
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::blocking::RequestBuilder {
        let mut r = self
            .client
            .request(method, self.url(path))
            .header("Authorization", &self.auth);
        if let Some(s) = &self.session {
            r = r.header("X-Emby-Token", &s.access_token);
        }
        r
    }
    fn decode<T: serde::de::DeserializeOwned>(
        response: reqwest::blocking::Response,
    ) -> Result<T, String> {
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "Jellyfin returned {status}. Check the connection and account permissions."
            ));
        }
        response
            .json()
            .map_err(|_| "The server returned an unexpected response.".into())
    }
    pub fn login(&mut self, name: &str, password: &str) -> Result<Session, String> {
        let response = self
            .request(reqwest::Method::POST, "Users/AuthenticateByName")
            .json(&json!({"Username":name,"Pw":password}))
            .send()
            .map_err(|_| {
                "Could not reach Jellyfin. Check the server address and network.".to_string()
            })?;
        let session: Session = Self::decode(response)?;
        self.session = Some(session.clone());
        Ok(session)
    }
    pub fn quick_start(&self) -> Result<Value, String> {
        Self::decode(
            self.request(reqwest::Method::POST, "QuickConnect/Initiate")
                .send()
                .map_err(|_| "Quick Connect could not reach the server.".to_string())?,
        )
    }
    pub fn quick_poll(&mut self, secret: &str) -> Result<Option<Session>, String> {
        let state: Value = Self::decode(
            self.request(reqwest::Method::GET, "QuickConnect/Connect")
                .query(&[("secret", secret)])
                .send()
                .map_err(|_| "Quick Connect connection failed.".to_string())?,
        )?;
        if state["Authenticated"].as_bool() != Some(true) {
            return Ok(None);
        }
        let session: Session = Self::decode(
            self.request(reqwest::Method::POST, "Users/AuthenticateWithQuickConnect")
                .json(&json!({"Secret":secret}))
                .send()
                .map_err(|_| "Quick Connect sign-in failed.".to_string())?,
        )?;
        self.session = Some(session.clone());
        Ok(Some(session))
    }
    #[cfg(test)]
    pub fn items(
        &self,
        section: &str,
        parent: Option<&str>,
        search: &str,
        offset: usize,
    ) -> Result<LibraryPage, String> {
        self.items_query(
            section,
            parent,
            search,
            offset,
            crate::browse::Options::default(),
        )
    }
    pub fn items_query(
        &self,
        section: &str,
        parent: Option<&str>,
        search: &str,
        offset: usize,
        options: crate::browse::Options,
    ) -> Result<LibraryPage, String> {
        let user = &self.session.as_ref().ok_or("Sign in first.")?.user.id;
        let path = if section == "Continue watching" {
            format!("Users/{user}/Items/Resume")
        } else {
            format!("Users/{user}/Items")
        };
        let offset = offset.to_string();
        let mut q = vec![
            ("StartIndex", offset.as_str()),
            ("Fields", "Overview,UserData"),
            ("Limit", "120"),
            ("EnableImages", "true"),
            ("Recursive", "true"),
        ];
        if let Some(parent) = parent {
            q.push(("ParentId", parent));
            q.push(("IncludeItemTypes", "Episode"));
            q.push(("SortBy", "ParentIndexNumber,IndexNumber"));
        } else {
            q.push((
                "IncludeItemTypes",
                if section == "Series" {
                    "Series"
                } else if section == "Movies" {
                    "Movie"
                } else if section == "Continue watching" {
                    "Movie,Episode"
                } else {
                    "Movie,Series"
                },
            ));
            let (sort, order) = if matches!(section, "Movies" | "Series") {
                options.sort.api()
            } else {
                ("DateCreated", "Descending")
            };
            q.push(("SortBy", sort));
            q.push(("SortOrder", order));
        }
        if section == "Favorites" {
            q.push(("Filters", "IsFavorite"));
        } else if let Some(filter) = options.filter.api() {
            q.push(("Filters", filter));
        }
        if !search.is_empty() {
            q.push(("SearchTerm", search));
        }
        Self::decode::<LibraryPage>(
            self.request(reqwest::Method::GET, &path)
                .query(&q)
                .send()
                .map_err(|_| "Could not load the library. Check your network.".to_string())?,
        )
    }
    pub fn seek_metadata(&self, id: &str) -> Result<Value, String> {
        let user = &self.session.as_ref().ok_or("Sign in first.")?.user.id;
        let value: Value = Self::decode(
            self.request(reqwest::Method::GET, &format!("Users/{user}/Items/{id}"))
                .timeout(Duration::from_secs(3))
                .send()
                .map_err(|_| "No seek metadata available.".to_string())?,
        )?;
        Ok(value)
    }
    pub fn seek_sheet(
        &self,
        id: &str,
        grid: &crate::seek_preview::Grid,
        index: u32,
    ) -> Result<Vec<u8>, String> {
        self.request(
            reqwest::Method::GET,
            &format!("Videos/{id}/Trickplay/{}/{index}.jpg", grid.width),
        )
        .query(&[("mediaSourceId", grid.media_source.as_str())])
        .timeout(Duration::from_secs(3))
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.bytes())
        .map(|b| b.to_vec())
        .map_err(|_| "No seek image available.".into())
    }
    pub fn item_details(&self, id: &str) -> Result<Item, String> {
        let user = &self.session.as_ref().ok_or("Sign in first.")?.user.id;
        Self::decode(
            self.request(reqwest::Method::GET, &format!("Users/{user}/Items/{id}"))
                .send()
                .map_err(|_| "Could not load title details.".to_string())?,
        )
    }
    pub fn logo(&self, id: &str) -> String {
        self.url(&format!("Items/{id}/Images/Logo?maxWidth=600&quality=90"))
            .to_string()
    }
    pub fn portrait(&self, id: &str) -> String {
        self.url(&format!(
            "Items/{id}/Images/Primary?maxWidth=180&quality=85"
        ))
        .to_string()
    }
    pub fn dashboard(&self) -> Result<Vec<(String, bool, Vec<Item>)>, String> {
        let user = &self.session.as_ref().ok_or("Sign in first.")?.user.id;
        let mut rows = Vec::new();
        for (title, path, extra) in [
            (
                "Continue Watching",
                format!("Users/{user}/Items/Resume"),
                vec![("IncludeItemTypes", "Movie,Episode")],
            ),
            (
                "Next Up",
                "Shows/NextUp".into(),
                vec![("UserId", user.as_str())],
            ),
            (
                "Latest in Movies",
                format!("Users/{user}/Items"),
                vec![
                    ("IncludeItemTypes", "Movie"),
                    ("SortBy", "DateCreated"),
                    ("SortOrder", "Descending"),
                ],
            ),
            (
                "Latest in Shows",
                format!("Users/{user}/Items"),
                vec![
                    ("IncludeItemTypes", "Series"),
                    ("SortBy", "DateCreated"),
                    ("SortOrder", "Descending"),
                ],
            ),
        ] {
            let mut q = vec![
                ("Limit", "20"),
                ("Recursive", "true"),
                ("Fields", "Overview,Genres,ChildCount"),
                ("EnableUserData", "true"),
            ];
            q.extend(extra);
            let page: LibraryPage = Self::decode(
                self.request(reqwest::Method::GET, &path)
                    .query(&q)
                    .send()
                    .map_err(|_| "Could not load the dashboard.".to_string())?,
            )?;
            rows.push((title.into(), rows.len() < 2, page.items));
        }
        let genres: Vec<String> = rows
            .iter()
            .skip(2)
            .flat_map(|(_, _, items)| items.iter().flat_map(|i| i.genres.clone()))
            .fold(Vec::new(), |mut acc, g| {
                if !acc.contains(&g) {
                    acc.push(g);
                }
                acc
            })
            .into_iter()
            .take(3)
            .collect();
        for genre in genres {
            let page: LibraryPage = Self::decode(
                self.request(reqwest::Method::GET, &format!("Users/{user}/Items"))
                    .query(&[
                        ("Limit", "20"),
                        ("Recursive", "true"),
                        ("IncludeItemTypes", "Movie,Series"),
                        ("Genres", genre.as_str()),
                        ("SortBy", "CommunityRating,SortName"),
                        ("SortOrder", "Descending"),
                        ("Fields", "Overview,Genres,ChildCount"),
                        ("EnableUserData", "true"),
                    ])
                    .send()
                    .map_err(|_| "Could not load genre shelves.".to_string())?,
            )?;
            rows.push((genre, false, page.items));
        }
        Ok(rows)
    }
    pub fn seasons(&self, series: &str, offset: usize) -> Result<LibraryPage, String> {
        let user = &self.session.as_ref().ok_or("Sign in first.")?.user.id;
        Self::decode(
            self.request(reqwest::Method::GET, &format!("Shows/{series}/Seasons"))
                .query(&[
                    ("UserId", user.as_str()),
                    ("Fields", "Overview,ChildCount"),
                    ("EnableUserData", "true"),
                    ("IsMissing", "false"),
                ])
                .send()
                .map_err(|_| "Could not load seasons.".to_string())?,
        )
        .map(|mut page: LibraryPage| {
            page.items
                .sort_by_key(|item| item.index_number.unwrap_or(u32::MAX));
            page.total_record_count = page.items.len();
            page.items = page.items.into_iter().skip(offset).take(120).collect();
            page
        })
    }
    pub fn episodes(
        &self,
        series: &str,
        season: Option<&str>,
        offset: usize,
    ) -> Result<LibraryPage, String> {
        let user = &self.session.as_ref().ok_or("Sign in first.")?.user.id;
        let offset = offset.to_string();
        let mut query = vec![
            ("UserId", user.as_str()),
            ("Fields", "Overview,ParentId"),
            ("EnableUserData", "true"),
            ("IsMissing", "false"),
            ("StartIndex", offset.as_str()),
            ("Limit", "120"),
        ];
        if let Some(season) = season {
            query.push(("SeasonId", season));
        }
        Self::decode(
            self.request(reqwest::Method::GET, &format!("Shows/{series}/Episodes"))
                .query(&query)
                .send()
                .map_err(|_| "Could not load episodes.".to_string())?,
        )
    }
    pub fn next_episode(&self, current: &Item) -> Result<Option<Item>, String> {
        if current.kind != "Episode" {
            return Ok(None);
        }
        let hydrated;
        let current = if current.series_id.is_none() {
            let user = &self.session.as_ref().ok_or("Sign in first.")?.user.id;
            hydrated = Self::decode::<Item>(
                self.request(
                    reqwest::Method::GET,
                    &format!("Users/{user}/Items/{}", current.id),
                )
                .send()
                .map_err(|_| "Could not look up this episode.".to_string())?,
            )?;
            &hydrated
        } else {
            current
        };
        let Some(series) = current.series_id.as_deref() else {
            return Ok(None);
        };
        let user = &self.session.as_ref().ok_or("Sign in first.")?.user.id;
        let mut offset = 0;
        loop {
            let offset_text = offset.to_string();
            let page: LibraryPage = Self::decode(
                self.request(reqwest::Method::GET, &format!("Shows/{series}/Episodes"))
                    .query(&[
                        ("UserId", user.as_str()),
                        ("StartItemId", current.id.as_str()),
                        ("StartIndex", offset_text.as_str()),
                        ("Limit", "120"),
                        ("IsMissing", "false"),
                        ("Fields", "Overview,ParentId"),
                        ("EnableUserData", "true"),
                    ])
                    .send()
                    .map_err(|_| "Could not find the next episode.".to_string())?,
            )?;
            let count = page.items.len();
            if let Some(next) = crate::series::following_episode(
                current,
                page.items.into_iter().skip(usize::from(offset == 0)),
            ) {
                return Ok(Some(next));
            }
            if count < 120 {
                return Ok(None);
            }
            offset += count;
        }
    }
    pub fn previous_episode(&self, current: &Item) -> Result<Option<Item>, String> {
        if current.kind != "Episode" {
            return Ok(None);
        }
        let hydrated;
        let current = if current.series_id.is_none() {
            let user = &self.session.as_ref().ok_or("Sign in first.")?.user.id;
            hydrated = Self::decode::<Item>(
                self.request(
                    reqwest::Method::GET,
                    &format!("Users/{user}/Items/{}", current.id),
                )
                .send()
                .map_err(|_| "Could not look up this episode.".to_string())?,
            )?;
            &hydrated
        } else {
            current
        };
        let Some(series) = current.series_id.as_deref() else {
            return Ok(None);
        };
        let mut offset = 0;
        let mut previous = None;
        loop {
            let page = self.episodes(series, None, offset)?;
            let count = page.items.len();
            for item in page.items {
                if item.id == current.id {
                    return Ok(previous);
                }
                if item.kind == "Episode"
                    && (item.parent_index_number == Some(0))
                        == (current.parent_index_number == Some(0))
                {
                    previous = Some(item);
                }
            }
            if count < 120 {
                return Ok(None);
            }
            offset += count;
        }
    }
    pub fn image(&self, item: &Item, backdrop: bool) -> String {
        let image_id = if item.kind == "Season" && !item.image_tags.contains_key("Primary") {
            item.series_id.as_deref().unwrap_or(&item.id)
        } else {
            &item.id
        };
        let mut url = self.url(&format!(
            "Items/{}/Images/{}",
            image_id,
            if backdrop && item.kind != "Episode" {
                "Backdrop/0"
            } else {
                "Primary"
            }
        ));
        url.query_pairs_mut()
            .append_pair("maxWidth", if backdrop { "1600" } else { "400" })
            .append_pair("quality", "85");
        // Jellyfin image endpoints support unauthenticated artwork; never put access tokens in URLs.
        url.to_string()
    }
    pub fn playback(&self, item: &Item) -> Result<PlaybackPlan, String> {
        let info: Value = Self::decode(self.request(reqwest::Method::POST,&format!("Items/{}/PlaybackInfo",item.id))
            .query(&[("UserId",&self.session.as_ref().ok_or("Sign in first.")?.user.id)])
            .json(&json!({"EnableDirectPlay":true,"EnableDirectStream":true,"EnableTranscoding":false}))
            .send().map_err(|_| "Could not prepare playback.".to_string())?)?;
        let source = info["MediaSources"]
            .as_array()
            .and_then(|v| v.first())
            .ok_or("No playable media source is available.")?;
        let mut url = self.url(&format!("Videos/{}/stream", item.id));
        url.query_pairs_mut().append_pair("static", "true");
        if let Some(id) = source["Id"].as_str() {
            url.query_pairs_mut().append_pair("MediaSourceId", id);
        }
        let mut subtitles = Vec::new();
        if let Some(streams) = source["MediaStreams"].as_array() {
            for stream in streams {
                if stream["Type"] != "Subtitle" || stream["IsExternal"] != true {
                    continue;
                }
                let subtitle_url = if let Some(delivery) = stream["DeliveryUrl"].as_str() {
                    self.subtitle_url(delivery)?
                } else {
                    let Some(index) = stream["Index"].as_u64() else {
                        continue;
                    };
                    let Some(source_id) = source["Id"].as_str() else {
                        continue;
                    };
                    let format = match stream["Codec"].as_str().unwrap_or("") {
                        "subrip" | "srt" => "srt",
                        "ass" | "ssa" => "ass",
                        "webvtt" | "vtt" => "vtt",
                        _ => continue,
                    };
                    self.url(&format!(
                        "Videos/{}/{source_id}/Subtitles/{index}/Stream.{format}",
                        item.id
                    ))
                };
                subtitles.push(ExternalSubtitle {
                    url: subtitle_url.to_string(),
                    title: stream["DisplayTitle"]
                        .as_str()
                        .or(stream["Title"].as_str())
                        .unwrap_or("External subtitles")
                        .into(),
                    language: stream["Language"].as_str().unwrap_or("und").into(),
                });
            }
        }
        Ok(PlaybackPlan {
            url: url.to_string(),
            subtitles,
        })
    }
    fn subtitle_url(&self, delivery: &str) -> Result<Url, String> {
        let mut url = self
            .base
            .join(
                if delivery.starts_with("//")
                    || delivery.starts_with("http://")
                    || delivery.starts_with("https://")
                {
                    delivery
                } else {
                    delivery.trim_start_matches('/')
                },
            )
            .map_err(|_| "Invalid subtitle URL.")?;
        if url.origin() != self.base.origin()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("The subtitle source belongs to another server.".into());
        }
        let query: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(key, _)| {
                !matches!(
                    key.to_ascii_lowercase().as_str(),
                    "api_key" | "token" | "accesstoken"
                )
            })
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        url.set_query(None);
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        Ok(url)
    }
    pub fn set_flag(&self, item: &Item, favorite: bool, value: bool) -> Result<(), String> {
        let user = &self.session.as_ref().ok_or("Sign in first.")?.user.id;
        let collection = if favorite {
            "FavoriteItems"
        } else {
            "PlayedItems"
        };
        let method = if value {
            reqwest::Method::POST
        } else {
            reqwest::Method::DELETE
        };
        self.request(method, &format!("Users/{user}/{collection}/{}", item.id))
            .send()
            .and_then(|r| r.error_for_status())
            .map(|_| ())
            .map_err(|_| "Could not update this title on Jellyfin.".into())
    }
    pub fn report(&self, event: &str, item: &Item, ticks: u64, paused: bool) -> Result<(), String> {
        self.request(reqwest::Method::POST,&format!("Sessions/Playing{event}"))
            .json(&json!({"ItemId":item.id,"PositionTicks":ticks,"IsPaused":paused,"CanSeek":true,"PlayMethod":"DirectStream"}))
            .send().and_then(|r|r.error_for_status()).map(|_|()).map_err(|_| "Watch progress could not be synced.".into())
    }
}
pub fn normalize_server(server: &str) -> Result<Url, String> {
    let mut url = Url::parse(server.trim())
        .map_err(|_| "Enter a full server URL, such as http://192.168.1.10:8096".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Use an http:// or https:// server URL without credentials.".into());
    }
    url.set_query(None);
    url.set_fragment(None);
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proxy_base_path_is_preserved() {
        assert_eq!(
            normalize_server("https://example.org/jellyfin")
                .unwrap()
                .join("Users")
                .unwrap()
                .as_str(),
            "https://example.org/jellyfin/Users"
        );
    }
    #[test]
    fn reject_invalid_servers() {
        for s in [
            "file:///etc/passwd",
            "ftp://example.org",
            "http://user:pass@example.org",
            "localhost:8096",
        ] {
            assert!(normalize_server(s).is_err(), "{s}");
        }
    }
    #[test]
    fn partial_metadata_is_valid() {
        let i: Item = serde_json::from_str(r#"{"Id":"abc","Name":"Film","Type":"Movie"}"#).unwrap();
        assert_eq!(i.user_data.playback_position_ticks, 0);
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    fn server(responses: Vec<(&'static str, &'static str)>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/jellyfin/", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            for (expected, response) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut data = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    data.extend_from_slice(&buffer[..n]);
                    if let Some(end) = data.windows(4).position(|b| b == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&data[..end]).to_lowercase();
                        let len = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .and_then(|s| s.parse::<usize>().ok())
                            .unwrap_or(0);
                        if data.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                let request = String::from_utf8(data).unwrap();
                let line = request.lines().next().unwrap();
                assert!(line.contains(expected), "Unexpected request: {line}");
                assert!(!line.contains("test-token"));
                assert!(
                    request
                        .to_lowercase()
                        .contains("authorization: mediabrowser")
                );
                if expected.contains("AuthenticateByName") {
                    assert!(request.contains(r#""Pw":"p@ssword""#));
                } else {
                    assert!(request.to_lowercase().contains("x-emby-token: test-token"));
                }
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
            }
        });
        (address, handle)
    }
    #[test]
    fn login_pagination_flags_and_progress_use_authenticated_requests() {
        let (address, handle) = server(vec![
            (
                "POST /jellyfin/Users/AuthenticateByName",
                r#"{"AccessToken":"test-token","User":{"Id":"user","Name":"Viewer"}}"#,
            ),
            (
                "GET /jellyfin/Users/user/Items?StartIndex=120",
                r#"{"Items":[{"Id":"film","Name":"A movie","Type":"Movie"}],"TotalRecordCount":121}"#,
            ),
            ("POST /jellyfin/Users/user/FavoriteItems/film", r#"{}"#),
            ("POST /jellyfin/Sessions/Playing/Progress", r#"{}"#),
        ]);
        let mut api = Api::new(&address, "test-device").unwrap();
        api.login("Viewer", "p@ssword").unwrap();
        let page = api.items("Movies", None, "", 120).unwrap();
        assert_eq!(page.total_record_count, 121);
        assert_eq!(page.items.len(), 1);
        api.set_flag(&page.items[0], true, true).unwrap();
        api.report("/Progress", &page.items[0], 123_000_000, true)
            .unwrap();
        assert!(!api.image(&page.items[0], false).contains("test-token"));
        handle.join().unwrap();
    }
    #[test]
    fn authentication_errors_are_actionable_and_do_not_echo_response_secrets() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0; 4096];
            let _ = s.read(&mut buf);
            s.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 12\r\nConnection: close\r\n\r\nsecret-token").unwrap();
        });
        let error = Api::new(&address, "test")
            .unwrap()
            .login("test", "wrong")
            .err()
            .unwrap();
        assert!(error.contains("401"));
        assert!(!error.contains("secret-token"));
        handle.join().unwrap();
    }
}

#[cfg(test)]
mod subtitle_url_tests {
    use super::*;
    #[test]
    fn external_subtitles_preserve_proxy_paths_without_url_credentials() {
        let api = Api::new("https://example.org/jellyfin/", "fixture").unwrap();
        let url = api
            .subtitle_url("/Videos/id/source/Subtitles/1/Stream.srt?api_key=fixture-secret&start=0")
            .unwrap();
        assert_eq!(
            url.path(),
            "/jellyfin/Videos/id/source/Subtitles/1/Stream.srt"
        );
        assert_eq!(url.query(), Some("start=0"));
        for value in [
            "https://other.example/sub.srt",
            "//other.example/sub.srt",
            "https://user:password@example.org/sub.srt",
        ] {
            assert!(api.subtitle_url(value).is_err());
        }
    }
}
