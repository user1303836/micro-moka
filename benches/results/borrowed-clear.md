| Key | Hasher | Capacity | Operation | Micro ns/op | vs v1.2 | Best peer | vs peer | Micro MAD % |
|---|---|---:|---|---:|---:|---|---:|---:|
| u64 | sip | 128 | clear-refill | 1568.43 | 0.982x | quick_cache | 1.078x | 12.6 |
| u64 | sip | 128 | clear-empty | 1.88 | 0.038x | lru | 3.113x | 0.5 |
| u64 | sip | 1024 | clear-refill | 14594.51 | 1.090x | quick_cache | 1.433x | 1.1 |
| u64 | sip | 1024 | clear-empty | 1.89 | 0.030x | lru | 3.202x | 0.5 |
| u64 | sip | 16384 | clear-refill | 204773.36 | 0.835x | quick_cache | 0.855x | 16.8 |
| u64 | sip | 16384 | clear-empty | 1.88 | 0.006x | lru | 3.188x | 0.4 |
| u64 | ahash | 128 | clear-refill | 713.61 | 0.952x | quick_cache | 0.904x | 0.4 |
| u64 | ahash | 128 | clear-empty | 1.90 | 0.038x | lru | 3.265x | 0.4 |
| u64 | ahash | 1024 | clear-refill | 5018.76 | 0.988x | quick_cache | 0.856x | 0.2 |
| u64 | ahash | 1024 | clear-empty | 1.93 | 0.031x | lru | 3.278x | 1.1 |
| u64 | ahash | 16384 | clear-refill | 91900.76 | 1.009x | quick_cache | 0.897x | 0.9 |
| u64 | ahash | 16384 | clear-empty | 1.95 | 0.003x | lru | 3.315x | 1.1 |
| string24 | sip | 128 | clear-refill | 3553.90 | 1.110x | quick_cache | 0.997x | 12.2 |
| string24 | sip | 128 | clear-empty | 2.40 | 0.048x | hashlink | 3.804x | 1.0 |
| string24 | sip | 1024 | clear-refill | 30522.10 | 1.000x | quick_cache | 0.963x | 5.7 |
| string24 | sip | 1024 | clear-empty | 2.67 | 0.051x | hashlink | 3.871x | 5.6 |
| string24 | sip | 16384 | clear-refill | 495795.45 | 1.026x | quick_cache | 0.962x | 2.2 |
| string24 | sip | 16384 | clear-empty | 2.38 | 0.007x | hashlink | 3.858x | 0.2 |
| string24 | ahash | 128 | clear-refill | 2490.00 | 0.974x | quick_cache | 0.958x | 0.5 |
| string24 | ahash | 128 | clear-empty | 2.35 | 0.047x | hashlink | 3.768x | 0.6 |
| string24 | ahash | 1024 | clear-refill | 21855.17 | 0.994x | quick_cache | 0.852x | 1.7 |
| string24 | ahash | 1024 | clear-empty | 2.50 | 0.049x | hashlink | 3.932x | 7.0 |
| string24 | ahash | 16384 | clear-refill | 407564.08 | 1.008x | quick_cache | 0.832x | 3.4 |
| string24 | ahash | 16384 | clear-empty | 2.35 | 0.006x | hashlink | 3.785x | 1.3 |
| string128 | sip | 128 | clear-refill | 5644.47 | 1.013x | quick_cache | 1.006x | 7.5 |
| string128 | sip | 128 | clear-empty | 2.40 | 0.048x | hashlink | 3.891x | 0.5 |
| string128 | sip | 1024 | clear-refill | 50021.25 | 1.007x | quick_cache | 0.975x | 2.1 |
| string128 | sip | 1024 | clear-empty | 2.40 | 0.049x | hashlink | 3.872x | 0.5 |
| string128 | sip | 16384 | clear-refill | 909416.67 | 1.006x | quick_cache | 1.001x | 3.3 |
| string128 | sip | 16384 | clear-empty | 2.42 | 0.004x | hashlink | 3.566x | 0.8 |
| string128 | ahash | 128 | clear-refill | 3486.21 | 1.012x | quick_cache | 1.010x | 8.8 |
| string128 | ahash | 128 | clear-empty | 2.35 | 0.046x | hashlink | 3.668x | 0.2 |
| string128 | ahash | 1024 | clear-refill | 35795.16 | 1.016x | quick_cache | 0.908x | 4.5 |
| string128 | ahash | 1024 | clear-empty | 2.34 | 0.044x | hashlink | 3.735x | 0.3 |
| string128 | ahash | 16384 | clear-refill | 627013.89 | 1.047x | quick_cache | 0.973x | 4.1 |
| string128 | ahash | 16384 | clear-empty | 2.34 | 0.004x | hashlink | 3.761x | 0.4 |

Lower ratios are better. +/-5% is labeled a tie, not statistical equivalence.
{'baseline_tie': 15, 'peer_loss': 20, 'baseline_win': 19, 'baseline_loss': 2, 'peer_win': 7, 'peer_tie': 9}
