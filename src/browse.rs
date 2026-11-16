#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Title,
    TitleDescending,
    Newest,
    RecentlyAdded,
    Rating,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Unwatched,
    Watched,
    Favorites,
}
#[derive(Clone, Copy, Default, Debug)]
pub struct Options {
    pub sort: Sort,
    pub filter: Filter,
}
impl Sort {
    pub const ALL: [Self; 5] = [
        Self::Title,
        Self::TitleDescending,
        Self::Newest,
        Self::RecentlyAdded,
        Self::Rating,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "Title · A–Z",
            Self::TitleDescending => "Title · Z–A",
            Self::Newest => "Release year",
            Self::RecentlyAdded => "Recently added",
            Self::Rating => "Rating",
        }
    }
    pub fn api(self) -> (&'static str, &'static str) {
        match self {
            Self::Title => ("SortName", "Ascending"),
            Self::TitleDescending => ("SortName", "Descending"),
            Self::Newest => ("ProductionYear", "Descending"),
            Self::RecentlyAdded => ("DateCreated", "Descending"),
            Self::Rating => ("CommunityRating", "Descending"),
        }
    }
}
impl Filter {
    pub const ALL: [Self; 4] = [Self::All, Self::Unwatched, Self::Watched, Self::Favorites];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All titles",
            Self::Unwatched => "Unwatched",
            Self::Watched => "Watched",
            Self::Favorites => "Favorites",
        }
    }
    pub fn api(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Unwatched => Some("IsUnplayed"),
            Self::Watched => Some("IsPlayed"),
            Self::Favorites => Some("IsFavorite"),
        }
    }
}
#[derive(Clone, Copy)]
pub enum Menu {
    Sort,
    Filter,
}
