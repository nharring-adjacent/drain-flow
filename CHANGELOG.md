# 0.5.2
## Updates
- Update version for anyhow, chrono, fraction, joinery, parking_lot, regex, tracing as well as serde and tracing-test in dev-dependencies
# 0.5.0-pre1

## Breaking Changes
- `SimpleDrain` moved into `drains::simple` module then renamed to `SingleLayer`

## Bugfixes and Improvements
- Standardized #[instrument] settings to use trace level and skip all large arguments
- Made formatting consistent and code more idiomatic