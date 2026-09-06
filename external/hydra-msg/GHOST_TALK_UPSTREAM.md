# Ghost Talk HYDRA upstream pin

Ghost Talk's vendored HYDRA integration tracks the public `hydra-msg` main-tree
steganography addition committed on 2026-08-31 as:

`a8b4b317cef85edce9d0f0528bb53125ebf6333b` (`feat: add stego`)

The native Ghost Talk integration uses HYDRA compact authenticated envelopes and
`hydra-stego::Stego`. The model-free `Deterministic` profile is available with no
external runtime. HYDRA's `FastUnicode`, `FastHybrid`, and `Arithmetic` profiles
require an explicitly configured local language-model backend; Ghost Talk keeps
those choices capability-gated rather than silently falling back to plaintext or
a different carrier.
