# vizir_text

`vizir_text` defines a small, renderer-agnostic text measurement interface used for chart guide
layout (axes, legends, titles).

- `no_std`-friendly (uses `alloc` for owned font family names)
- Designed to be implemented by shaping engines (e.g. Parley) or web canvas measurement
