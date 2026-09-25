# Kaspa-carried voice composition example

This application-layer example demonstrates the Ghost Talk boundary: produce a complete `ghost_talk::VoiceChunk`, serialize it, and only then hand those opaque bytes to the host's Kaspa/HYDRA transport composition. Transport fragmentation, encryption, fees, and transaction planning remain outside the voice SDK.
