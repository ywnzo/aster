use crate::jellyfin::Item;
#[derive(Clone, Default)]
pub enum Route {
    #[default]
    Library,
    Seasons {
        series: Box<Item>,
    },
    Episodes {
        series: Box<Item>,
        season: Option<Box<Item>>,
    },
}
impl Route {
    pub fn is_library(&self) -> bool {
        matches!(self, Self::Library)
    }
    pub fn is_episodes(&self) -> bool {
        matches!(self, Self::Episodes { .. })
    }
    pub fn key(&self) -> String {
        match self {
            Self::Library => "library".into(),
            Self::Seasons { series } => format!("seasons:{}", series.id),
            Self::Episodes { series, season } => format!(
                "episodes:{}:{}",
                series.id,
                season.as_ref().map(|s| s.id.as_str()).unwrap_or("all")
            ),
        }
    }
    pub fn title(&self) -> Option<String> {
        match self {
            Self::Library => None,
            Self::Seasons { series } => Some(series.name.clone()),
            Self::Episodes { series, season } => Some(format!(
                "{} · {}",
                series.name,
                season
                    .as_ref()
                    .map(|s| s.season_label())
                    .unwrap_or_else(|| "All episodes".into())
            )),
        }
    }
}
#[derive(Clone)]
pub struct Snapshot {
    pub route: Route,
    pub items: Vec<Item>,
    pub focus: usize,
    pub total: usize,
    pub search: String,
    pub detail: Option<Item>,
    pub list_view: bool,
}
/// StartItemId includes the current item (or its primary version). Normal
/// playback never inserts specials; explicit specials stay within specials.
pub fn following_episode(current: &Item, items: impl IntoIterator<Item = Item>) -> Option<Item> {
    let special = current.parent_index_number == Some(0);
    items.into_iter().find(|item| {
        item.kind == "Episode"
            && item.id != current.id
            && if special {
                item.parent_index_number == Some(0)
            } else {
                item.parent_index_number != Some(0)
            }
    })
}
impl Route {
    pub fn fetch(
        &self,
        api: &crate::jellyfin::Api,
        section: &str,
        search: &str,
        offset: usize,
        options: crate::browse::Options,
    ) -> Result<crate::jellyfin::LibraryPage, String> {
        match self {
            Self::Library => api.items_query(section, None, search, offset, options),
            Self::Seasons { series } => api.seasons(&series.id, offset).map(|mut page| {
                for item in &mut page.items {
                    item.series_id.get_or_insert_with(|| series.id.clone());
                    item.series_name.get_or_insert_with(|| series.name.clone());
                }
                page
            }),
            Self::Episodes { series, season } => api
                .episodes(&series.id, season.as_ref().map(|s| s.id.as_str()), offset)
                .map(|mut page| {
                    for item in &mut page.items {
                        item.series_id.get_or_insert_with(|| series.id.clone());
                        item.series_name.get_or_insert_with(|| series.name.clone());
                        if let Some(season) = season {
                            item.season_id.get_or_insert_with(|| season.id.clone());
                            if item.parent_index_number.is_none() {
                                item.parent_index_number = season.index_number;
                            }
                        }
                    }
                    page
                }),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn episode(id: &str, s: u32, e: u32) -> Item {
        Item {
            id: id.into(),
            kind: "Episode".into(),
            parent_index_number: Some(s),
            index_number: Some(e),
            ..Item::default()
        }
    }
    #[test]
    fn season_boundaries_and_specials_do_not_depend_on_consecutive_episode_numbers() {
        let current = episode("final", 1, 24);
        let next =
            following_episode(&current, [episode("special", 0, 7), episode("next", 2, 3)]).unwrap();
        assert_eq!(next.id, "next");
    }
    #[test]
    fn specials_remain_separate_and_current_episode_never_loops() {
        let current = episode("special", 0, 1);
        assert!(following_episode(&current, [current.clone(), episode("regular", 1, 1)]).is_none());
        assert_eq!(
            following_episode(&current, [episode("next", 0, 8)])
                .unwrap()
                .id,
            "next"
        );
    }
}
