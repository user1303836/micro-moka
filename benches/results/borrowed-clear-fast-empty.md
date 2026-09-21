| Key | Hasher | Capacity | Operation | Micro ns/op | vs v1.2 | Best peer | vs peer | Micro MAD % |
|---|---|---:|---|---:|---:|---|---:|---:|
| u64 | sip | 128 | clear-refill | 866.01 | 0.671x | quick_cache | 0.702x | 0.6 |
| u64 | sip | 128 | clear-empty | 0.38 | 0.008x | hashlink | 0.626x | 4.2 |
| u64 | sip | 1024 | clear-refill | 7511.51 | 0.689x | quick_cache | 0.708x | 2.9 |
| u64 | sip | 1024 | clear-empty | 0.39 | 0.006x | hashlink | 0.607x | 3.1 |
| u64 | sip | 16384 | clear-refill | 115745.27 | 0.524x | quick_cache | 0.504x | 0.7 |
| u64 | sip | 16384 | clear-empty | 0.37 | 0.001x | lru | 0.613x | 1.1 |
| u64 | ahash | 128 | clear-refill | 713.49 | 0.968x | quick_cache | 0.922x | 0.2 |
| u64 | ahash | 128 | clear-empty | 0.37 | 0.006x | lru | 0.642x | 0.2 |
| u64 | ahash | 1024 | clear-refill | 4878.13 | 0.969x | quick_cache | 0.828x | 0.2 |
| u64 | ahash | 1024 | clear-empty | 0.37 | 0.006x | lru | 0.631x | 1.5 |
| u64 | ahash | 16384 | clear-refill | 90151.05 | 1.018x | quick_cache | 0.888x | 1.1 |
| u64 | ahash | 16384 | clear-empty | 0.37 | 0.001x | lru | 0.645x | 1.4 |
| string24 | sip | 128 | clear-refill | 3042.68 | 0.989x | quick_cache | 0.943x | 0.5 |
| string24 | sip | 128 | clear-empty | 0.37 | 0.008x | hashlink | 0.600x | 0.6 |
| string24 | sip | 1024 | clear-refill | 26329.82 | 1.002x | quick_cache | 0.961x | 0.8 |
| string24 | sip | 1024 | clear-empty | 0.37 | 0.008x | hashlink | 0.606x | 0.4 |
| string24 | sip | 16384 | clear-refill | 466670.45 | 1.018x | quick_cache | 0.969x | 0.9 |
| string24 | sip | 16384 | clear-empty | 0.37 | 0.001x | hashlink | 0.594x | 0.4 |
| string24 | ahash | 128 | clear-refill | 2466.20 | 0.980x | quick_cache | 0.967x | 0.5 |
| string24 | ahash | 128 | clear-empty | 0.37 | 0.008x | hashlink | 0.603x | 0.6 |
| string24 | ahash | 1024 | clear-refill | 21803.08 | 1.018x | quick_cache | 0.905x | 0.7 |
| string24 | ahash | 1024 | clear-empty | 0.37 | 0.007x | hashlink | 0.602x | 0.2 |
| string24 | ahash | 16384 | clear-refill | 368613.14 | 0.998x | quick_cache | 0.774x | 1.2 |
| string24 | ahash | 16384 | clear-empty | 0.37 | 0.001x | hashlink | 0.602x | 0.5 |
| string128 | sip | 128 | clear-refill | 5230.17 | 0.943x | quick_cache | 0.979x | 1.8 |
| string128 | sip | 128 | clear-empty | 0.37 | 0.007x | hashlink | 0.602x | 0.3 |
| string128 | sip | 1024 | clear-refill | 49958.34 | 1.006x | quick_cache | 0.982x | 4.5 |
| string128 | sip | 1024 | clear-empty | 0.37 | 0.008x | hashlink | 0.599x | 0.1 |
| string128 | sip | 16384 | clear-refill | 815660.71 | 1.011x | quick_cache | 0.977x | 3.2 |
| string128 | sip | 16384 | clear-empty | 0.37 | 0.001x | hashlink | 0.598x | 1.0 |
| string128 | ahash | 128 | clear-refill | 2896.97 | 0.987x | quick_cache | 0.932x | 0.6 |
| string128 | ahash | 128 | clear-empty | 0.37 | 0.007x | hashlink | 0.600x | 0.4 |
| string128 | ahash | 1024 | clear-refill | 32080.66 | 1.015x | quick_cache | 0.955x | 3.7 |
| string128 | ahash | 1024 | clear-empty | 0.37 | 0.008x | hashlink | 0.602x | 0.9 |
| string128 | ahash | 16384 | clear-refill | 582463.00 | 0.946x | quick_cache | 0.928x | 6.8 |
| string128 | ahash | 16384 | clear-empty | 0.37 | 0.001x | hashlink | 0.599x | 0.3 |

Lower ratios are better. +/-5% is labeled a tie, not statistical equivalence.
{'baseline_win': 23, 'peer_win': 29, 'baseline_tie': 13, 'peer_tie': 7}
