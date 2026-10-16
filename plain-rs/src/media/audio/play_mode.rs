#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(feature = "graphql", derive(async_graphql::Enum))]
#[cfg_attr(feature = "graphql", graphql(rename_items = "SCREAMING_SNAKE_CASE"))]
pub enum MediaPlayMode {
    Repeat,
    RepeatOne,
    Shuffle,
}
impl MediaPlayMode {
    pub fn next(self) -> Self {
        match self {
            Self::Repeat => Self::RepeatOne,
            Self::RepeatOne => Self::Shuffle,
            Self::Shuffle => Self::Repeat,
        }
    }

    pub fn cycle(prefs: &crate::prefs::Prefs) -> crate::error::DomainResult<Self> {
        let next = prefs
            .update_user(|entries| {
                let key = crate::prefs::user::keys::AUDIO_PLAY_MODE;
                let ordinal: i32 = serde_json::from_value(
                    entries.get(key).cloned().unwrap_or(serde_json::json!(0)),
                )
                .map_err(crate::prefs::PrefsError::Decode)?;
                let next = Self::from_ordinal(ordinal).map(Self::next);
                if let Some(mode) = next {
                    entries.insert(key.into(), serde_json::json!(mode.ordinal()));
                }
                Ok(next)
            })
            .map_err(|error| crate::error::DomainError::Other(error.to_string()))?;
        next.ok_or_else(|| crate::error::DomainError::Other("invalid audio play mode".into()))
    }

    pub fn ordinal(self) -> i32 {
        match self {
            Self::Repeat => 0,
            Self::RepeatOne => 1,
            Self::Shuffle => 2,
        }
    }
    pub fn from_ordinal(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Repeat),
            1 => Some(Self::RepeatOne),
            2 => Some(Self::Shuffle),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Repeat => "REPEAT",
            Self::RepeatOne => "REPEAT_ONE",
            Self::Shuffle => "SHUFFLE",
        }
    }
    pub fn from_name(value: &str) -> Option<Self> {
        match value {
            "REPEAT" => Some(Self::Repeat),
            "REPEAT_ONE" => Some(Self::RepeatOne),
            "SHUFFLE" => Some(Self::Shuffle),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/media/audio/play_mode.rs"]
mod tests;
