## What this changes

<!-- One or two sentences. Link the issue: Fixes #123 -->

## How I tested it

<!-- Windows version, steps, and what you checked. -->

## Checklist

- [ ] Every commit is signed off (`git commit -s`)
- [ ] Tests added or updated
- [ ] Documentation updated if behaviour changed
- [ ] Interface text is plain, calm and sentence case

### Only if this touches cleanup rules or the safety layer

- [ ] I linked documentation showing the location is regenerated automatically
- [ ] Tests prove the rule can't reach Protected paths, including through links, junctions and OneDrive folders
- [ ] `CLEANUP_RULES.md` and `rules/cleanup-rules.toml` are updated together
- [ ] Listed under **Cleanup rules** in `CHANGELOG.md`
