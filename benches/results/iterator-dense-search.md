| Key | Hasher | Capacity | Operation | Micro ns/op | vs v1.2 | Best peer | vs peer | Micro MAD % |
|---|---|---:|---|---:|---:|---|---:|---:|
| u64 | sip | 128 | iter-dense | 39.25 | 1.004x | quick_cache | 1.356x | 0.4 |
| u64 | sip | 1024 | iter-dense | 272.72 | 0.992x | quick_cache | 1.099x | 0.2 |
| u64 | sip | 16384 | iter-dense | 4670.84 | 0.993x | quick_cache | 0.875x | 0.5 |
| u64 | ahash | 128 | iter-dense | 39.68 | 1.031x | quick_cache | 1.372x | 0.2 |
| u64 | ahash | 1024 | iter-dense | 277.93 | 1.000x | quick_cache | 1.123x | 0.2 |
| u64 | ahash | 16384 | iter-dense | 4651.58 | 1.000x | quick_cache | 0.876x | 2.2 |
| string24 | sip | 128 | iter-dense | 38.71 | 1.018x | quick_cache | 1.301x | 0.7 |
| string24 | sip | 1024 | iter-dense | 274.73 | 1.006x | quick_cache | 1.061x | 0.1 |
| string24 | sip | 16384 | iter-dense | 6084.79 | 0.998x | quick_cache | 0.877x | 0.2 |
| string24 | ahash | 128 | iter-dense | 39.39 | 1.031x | quick_cache | 1.351x | 0.2 |
| string24 | ahash | 1024 | iter-dense | 270.89 | 0.993x | quick_cache | 1.066x | 0.9 |
| string24 | ahash | 16384 | iter-dense | 6098.96 | 0.991x | quick_cache | 0.872x | 0.2 |
| string128 | sip | 128 | iter-dense | 39.19 | 1.008x | quick_cache | 1.317x | 0.5 |
| string128 | sip | 1024 | iter-dense | 275.35 | 0.995x | quick_cache | 1.076x | 0.8 |
| string128 | sip | 16384 | iter-dense | 6126.35 | 1.005x | quick_cache | 0.881x | 0.8 |
| string128 | ahash | 128 | iter-dense | 39.26 | 1.029x | quick_cache | 1.351x | 0.5 |
| string128 | ahash | 1024 | iter-dense | 273.59 | 0.996x | quick_cache | 1.063x | 0.5 |
| string128 | ahash | 16384 | iter-dense | 6142.28 | 1.004x | quick_cache | 0.879x | 0.8 |

Lower ratios are better. +/-5% is labeled a tie, not statistical equivalence.
{'baseline_tie': 18, 'peer_loss': 12, 'peer_win': 6}
