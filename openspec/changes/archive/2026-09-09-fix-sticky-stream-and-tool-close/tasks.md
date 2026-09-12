- [x] Regress the past-end sticky offset and same-layout geometry contract.
- [x] Fix sticky coordinate sampling and remove obsolete detail close retention.
- [x] Verify native streaming across multiple turns and closing tool details without a later stream event.
- [x] Run UI tests/build, update DOX and validate/archive.

Evidence:
- User video at 00:03–00:05 shows current/previous prompt alternation; native tracing reproduced the mixed pre/post-layout offsets.
- Corrected native run retained the current turn for 1,494 consecutive sticky decisions during streaming.
- Native completed-tool close captured with its header visible and payload absent; click, focus check and screenshot together took 804ms. This is an end-to-end observation, not a frame-latency benchmark.
- `cargo test -p zeron-ui`: 1,215 passed. Final attach-reset change: 11 focused sticky tests passed. Build, formatting and diff checks passed.
