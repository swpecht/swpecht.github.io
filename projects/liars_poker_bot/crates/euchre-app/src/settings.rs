use dioxus::prelude::*;
use log::debug;

/// Register all settings data to default
pub fn register_settings<T>(cx: Scope<T>) {
    use_shared_state_provider(cx, || MinPlayers(1));
    use_shared_state_provider(cx, || EventId { id: "".to_string() });
}

struct PlayerId {
    id: usize,
}

impl From<PlayerId> for usize {
    fn from(value: PlayerId) -> Self {
        value.id
    }
}

pub fn player_id<T>(cx: Scope<T>) -> Option<usize> {
    use_shared_state::<PlayerId>(cx).map(|x| x.read().id)
}

pub fn set_player_id(cx: Scope, id: usize) {
    use_shared_state_provider(cx, || PlayerId { id });
}

struct EventId {
    id: String,
}

/// Returns the current event id if one has been set
pub fn event_id<T>(cx: Scope<T>) -> Option<String> {
    use_shared_state::<EventId>(cx).map(|x| x.read().id.clone())
}

pub fn set_event_id<T>(cx: Scope<T>, id: String) {
    use_shared_state::<EventId>(cx)
        .expect("setting not found. did you register settings?")
        .write()
        .id = id;
}

struct MinPlayers(usize);

pub fn min_players<T>(cx: Scope<T>) -> usize {
    use_shared_state::<MinPlayers>(cx)
        .map(|x| x.read().0)
        .expect("settings not found. did you register settings?")
}

pub fn set_min_players<T>(cx: Scope<T>, min_players: usize) {
    debug!("setting min players: {}", min_players);
    let s = use_shared_state::<MinPlayers>(cx).unwrap();
    s.write().0 = min_players;
}
