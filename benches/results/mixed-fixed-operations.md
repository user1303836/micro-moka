| Key | Hasher | Capacity | Operation | Micro ns/op | vs v1.2 | Best peer | vs peer | Micro MAD % |
|---|---|---:|---|---:|---:|---|---:|---:|
| u64 | sip | 128 | mixed-95 | 14.08 | 0.951x | hashlink | 1.386x | 8.4 |
| u64 | sip | 1024 | mixed-95 | 15.21 | 0.980x | hashlink | 1.475x | 3.7 |
| u64 | sip | 16384 | mixed-95 | 22.39 | 1.031x | quick_cache | 1.897x | 4.0 |
| u64 | ahash | 128 | mixed-95 | 2.70 | 1.002x | hashlink | 0.890x | 1.0 |
| u64 | ahash | 1024 | mixed-95 | 2.71 | 0.992x | hashlink | 0.855x | 0.7 |
| u64 | ahash | 16384 | mixed-95 | 4.17 | 0.955x | quick_cache | 0.836x | 4.0 |
| string24 | sip | 128 | mixed-95 | 16.48 | 0.989x | lru | 0.892x | 0.2 |
| string24 | sip | 1024 | mixed-95 | 18.04 | 0.992x | hashlink | 0.905x | 0.6 |
| string24 | sip | 16384 | mixed-95 | 21.15 | 0.991x | hashlink | 0.987x | 0.8 |
| string24 | ahash | 128 | mixed-95 | 9.00 | 1.010x | hashlink | 1.078x | 0.6 |
| string24 | ahash | 1024 | mixed-95 | 10.24 | 1.002x | hashlink | 0.932x | 1.3 |
| string24 | ahash | 16384 | mixed-95 | 12.70 | 0.951x | quick_cache | 0.946x | 4.2 |
| string128 | sip | 128 | mixed-95 | 34.83 | 1.000x | quick_cache | 0.906x | 0.4 |
| string128 | sip | 1024 | mixed-95 | 39.12 | 0.993x | hashlink | 0.946x | 1.3 |
| string128 | sip | 16384 | mixed-95 | 46.24 | 0.993x | quick_cache | 0.979x | 2.7 |
| string128 | ahash | 128 | mixed-95 | 15.94 | 0.999x | hashlink | 0.937x | 0.3 |
| string128 | ahash | 1024 | mixed-95 | 20.70 | 1.036x | hashlink | 0.994x | 1.9 |
| string128 | ahash | 16384 | mixed-95 | 26.51 | 1.006x | lru | 1.055x | 1.0 |

Lower ratios are better. +/-5% is labeled a tie, not statistical equivalence.
{'baseline_tie': 18, 'peer_loss': 5, 'peer_win': 10, 'peer_tie': 3}
