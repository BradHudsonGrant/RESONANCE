# Staging

Budget: the staging diff for one track opens in under 50 ms ([architecture.md](../architecture.md#performance-budgets)).

Machine: ROG Zephyrus G16, Core Ultra 9 285H (16 logical cores), 64 GB RAM, Windows 11 Pro. Real-time antivirus scanning is on and the repo has no exclusion.

## Method

A copy of the 100,000-track library from `just synthetic-library`, opened by the debug build of `resonance serve`, which migrated it to schema 10. Five proposals each (title, label, catalogue number, year, mix) were written for 1,000 random tracks. A Node 24 script timed each command from send to reply over the local WebSocket, one call at a time, after 20 warm-up calls. The times include the socket round trip and JSON on both sides.

## Results

| Command | Median | 95th percentile | Max | Calls |
| --- | --- | --- | --- | --- |
| `staging_diff` | 0.56 ms | 17.34 ms | 19.86 ms | 500 |
| `staging_queue`, 50 rows | 1.56 ms | 18.47 ms | 21.43 ms | 50 |
| `accept_proposed_fields`, one field | 2.35 ms | 20.28 ms | 27.66 ms | 100 |
| `set_user_tag`, one track | 1.95 ms | 19.07 ms | 40.46 ms | 100 |
| `accept_all_above_threshold` at 0.9, 1,000 tracks with proposals | 171.5 ms | | | 1 |

## Conclusion

The staging diff meets its budget on the debug build, with the slowest of 500 calls at 19.86 ms. Accepting a field or setting a user value, which rewrites the asset's sort and search columns, also stays under 50 ms. Accepting everything above a threshold across 1,000 tracks runs in one transaction in under 200 ms.
