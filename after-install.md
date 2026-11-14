# Puritanical for Hermes installed

Enable it if you did not install with `--enable`:

```bash
hermes plugins enable puritanical
```

Restart Hermes or the gateway after enabling.

In shared gateways, restrict `/puritanical` to trusted users with Hermes slash-command access controls. Each session keeps its own level where Hermes passes the calling session to plugin commands; on versions that do not, one level serves every session in the process.

Commands:

- `/puritanical [lite|full|ultra|off]`
- `/puritanical default [lite|full|ultra|off]`: the level new sessions start in
- `/puritanical-review [target]`
- `/puritanical-audit [target]`
- `/puritanical-debt`
- `/puritanical-help`

Bundled skills are available as `puritanical:puritanical`, `puritanical:puritanical-review`, `puritanical:puritanical-audit`, `puritanical:puritanical-debt`, and `puritanical:puritanical-help`.
