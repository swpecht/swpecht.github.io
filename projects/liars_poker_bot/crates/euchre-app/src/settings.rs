use dioxus::prelude::*;
use log::debug;
use rand::{thread_rng, Rng};

const PLAYER_ID_KEY: &str = "PLAYER_ID";

/// Register all settings data to default
pub fn register_settings<T>(cx: Scope<T>) {
    debug!("registering settings shared state...");
    use_shared_state_provider(cx, || MinPlayers(1));
    use_shared_state_provider(cx, || EventId { id: "".to_string() });

    let local_storage = web_sys::window().unwrap().local_storage().unwrap().unwrap();
    let stored_id = local_storage.get_item(PLAYER_ID_KEY);

    let player_id: usize = match stored_id.map(|x| x.map(|y| y.parse())) {
        Ok(Some(Ok(x))) => x,
        _ => {
            debug!("failed to read previously saved id, generating a new one");
            let id: usize = thread_rng().gen();
            local_storage
                .set_item(PLAYER_ID_KEY, id.to_string().as_str())
                .expect("error storing player id");

            id
        }
    };

    use_shared_state_provider(cx, || PlayerId { id: player_id });
}

struct PlayerId {
    id: usize,
}

impl From<PlayerId> for usize {
    fn from(value: PlayerId) -> Self {
        value.id
    }
}

pub fn get_player_id<T>(cx: Scope<T>) -> Option<usize> {
    use_shared_state::<PlayerId>(cx).map(|x| x.read().id)
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

    // We do a silent write to avoid re-rendering things
    s.write_silent().0 = min_players;
}
