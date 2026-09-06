# Presenter room
Use `ghost_rooms::ChannelPolicy::PresentersOnly`. Audience membership permits listening but `Room::can_send_*` rejects sending; production group builds additionally enforce sender role cryptographically in HYDRA group state.
