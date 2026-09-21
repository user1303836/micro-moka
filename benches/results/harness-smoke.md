| Key | Hasher | Capacity | Operation | Micro ns/op | vs v1.2 | Best peer | vs peer | Micro MAD % |
|---|---|---:|---|---:|---:|---|---:|---:|
| u64 | sip | 32 | load-hit | 8.06 | 0.997x | hashlink | 1.341x | 0.0 |
| u64 | sip | 32 | load-cycle | 15.76 | 1.006x | quick_cache | 0.848x | 0.0 |
| u64 | ahash | 32 | load-hit | 2.23 | 1.037x | quick_cache | 0.725x | 0.0 |
| u64 | ahash | 32 | load-cycle | 11.27 | 0.999x | quick_cache | 1.404x | 0.0 |
| string24 | sip | 32 | load-hit | 21.74 | 1.008x | hashlink | 2.275x | 0.0 |
| string24 | sip | 32 | load-cycle | 39.33 | 0.988x | quick_cache | 1.534x | 0.0 |
| string24 | ahash | 32 | load-hit | 15.90 | 0.979x | hashlink | 2.776x | 0.0 |
| string24 | ahash | 32 | load-cycle | 33.14 | 1.005x | quick_cache | 1.995x | 0.0 |
| string128 | sip | 32 | load-hit | 39.04 | 1.016x | hashlink | 1.356x | 0.0 |
| string128 | sip | 32 | load-cycle | 59.57 | 0.981x | quick_cache | 1.170x | 0.0 |
| string128 | ahash | 32 | load-hit | 23.21 | 1.007x | hashlink | 1.953x | 0.0 |
| string128 | ahash | 32 | load-cycle | 36.94 | 0.994x | quick_cache | 1.571x | 0.0 |

Lower ratios are better. +/-5% is labeled a tie, not statistical equivalence.
{'baseline_tie': 12, 'peer_loss': 10, 'peer_win': 2}
