# Direct chat example
Encode a `ChatEvent`, pass it to HYDRA, then feed each opaque HYDRA envelope to `ghost_protocol::fragment` for Kaspa carriage. On receive, reassemble GHST first, then authenticate/decrypt in HYDRA, then decode `ChatEvent`.
