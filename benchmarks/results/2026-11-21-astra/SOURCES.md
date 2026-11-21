# Sources and licences

The Puritanical skill body is from this fork at `64de6ae`. The comparison uses the default coding skill from each project, with its YAML frontmatter removed. The exact UTF-8 bodies are in [rules.json](rules.json). The reference revisions are [Chisle at bda2483](https://github.com/JayPokale/Chisle/tree/bda2483c27e66f84e16b7b1ece9cfaf74c839753) and [Ponytail at 96608e0](https://github.com/DietrichGebert/ponytail/tree/96608e078b2d5ea607c5c3b8b5a54d1c1fba6846).

The former Puritanical repository at `1d95a22d50f7c7b17458ac6ba2b085f9f37fe55b` supplied the CSV and matched pagination tasks. The atomic inventory fixture and skipped-row diagnostic come from its local follow-up experiment. This directory includes the selected fixture data and original checker functions so replay works after replacement of that repository. The original terms and notices accompany the extracts. [Retired licence](LICENSE-retired.txt) · [Retired notices](NOTICE-retired.txt)

The currency reuse and field rename tasks use the prompts, starter files and checker functions from the pinned Ponytail revision. The JavaScript tasks use the duplicate display paths and standard-library fixtures already shipped in the fork. [Ponytail tasks](https://github.com/DietrichGebert/ponytail/blob/96608e078b2d5ea607c5c3b8b5a54d1c1fba6846/benchmarks/agentic/tasks.py) · [Ponytail licence](LICENSE-ponytail.txt) · [JavaScript fixtures](../../agentic/fixtures) · [Repository licence](../../../LICENSE)

## Generation

Each attempt used a new Codex CLI 0.171.0 process on Linux with the registered prompt as UTF-8 on standard input. The working directory was an empty temporary filesystem. Bubblewrap mounted a blank file over the user's global `AGENTS.md` for that process. The three arms used the same signed-in plan, medium reasoning effort and tool restrictions. The runner supplied the model slug explicitly. The CLI's JSONL usage event does not echo the model slug.

```sh
codex exec --ignore-user-config --ignore-rules --ephemeral --skip-git-repo-check \
  --sandbox read-only --model gpt-6.2-astra --json \
  -c 'model_reasoning_effort="medium"' -c project_doc_max_bytes=0 \
  -c skills.include_instructions=false -c 'developer_instructions=""' \
  -c 'web_search="disabled"' --enable skip_host_skill_discovery \
  --disable shell_tool --disable unified_exec --disable apps --disable plugins \
  --disable hooks --disable memories --disable multi_agent --disable shell_snapshot \
  --output-last-message "$answer_file" - < "$prompt_file" > "$events_file"
```

The runner allowed 240 seconds per attempt and up to 3 concurrent requests. It checked each saved answer against the last assistant message and retained the `turn.completed.usage` values. `input_tokens` includes `cached_input_tokens`. The total is input plus output. The startup warning for experimental skill discovery appears as an error item even on successful turns. Text criteria for the secondary CSV notice check are heuristic: a generic message mentioning a row can match. The primary CSV verdict measures the returned totals on clean and imperfect input.
