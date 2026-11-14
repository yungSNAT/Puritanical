#!/usr/bin/env node
// Hermes support is a real plugin, not just copied rules: the repo root must be
// installable with `hermes plugins install owner/repo`, register bundled skills,
// inject active mode context, and expose slash commands.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawnSync } = require('child_process');
const { filterSkillBodyForMode } = require('../hooks/puritanical-instructions');

const commands = ['puritanical', 'puritanical-review', 'puritanical-audit', 'puritanical-debt', 'puritanical-help'];

const root = path.join(__dirname, '..');

// Probe once; on Windows `python3` is the Store-alias stub that fails
// even when Python is installed, so fall back to `python` (mirrors benchmarks/correctness.js).
let pythonCmd;
function pythonExe() {
  if (pythonCmd) return pythonCmd;
  for (const cmd of ['python3', 'python']) {
    const probe = spawnSync(cmd, ['-c', 'import sys'], { encoding: 'utf8', timeout: 30000 });
    if (probe.error && probe.error.code !== 'ENOENT') throw probe.error;
    if (probe.status === 0) {
      pythonCmd = cmd;
      return pythonCmd;
    }
  }
  pythonCmd = 'python3';
  return pythonCmd;
}

function python(script, env = {}) {
  const result = spawnSync(pythonExe(), ['-c', script], {
    cwd: root,
    // Isolate from the developer's own ~/.config/puritanical and PURITANICAL_DEFAULT_MODE.
    env: {
      ...process.env,
      PURITANICAL_DEFAULT_MODE: '',
      XDG_CONFIG_HOME: fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-config-')),
      ...env,
    },
    encoding: 'utf8',
    timeout: 30000,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`python failed\nSTDOUT:\n${result.stdout}\nSTDERR:\n${result.stderr}`);
  }
  return result.stdout.trim();
}

test('Hermes plugin manifest matches runtime skills, hooks, commands, and package version', () => {
  const manifestPath = path.join(root, 'plugin.yaml');
  assert.ok(fs.existsSync(manifestPath), 'missing root plugin.yaml');
  const manifest = fs.readFileSync(manifestPath, 'utf8');
  const packageJson = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
  const skillDirs = fs.readdirSync(path.join(root, 'skills'))
    .filter((name) => fs.existsSync(path.join(root, 'skills', name, 'SKILL.md')))
    .sort();

  assert.match(manifest, /^name:\s*puritanical$/m);
  assert.match(manifest, new RegExp(`^version:\\s*${packageJson.version}$`, 'm'));
  assert.match(manifest, new RegExp(`^author:\\s*${packageJson.author.name}$`, 'm'));
  assert.deepEqual(commands.filter((name) => manifest.includes(`  - ${name}`)), commands);
  assert.deepEqual(skillDirs.filter((name) => manifest.includes(`  - ${name}`)), skillDirs);
  assert.match(manifest, /pre_llm_call/);
  assert.match(manifest, /pre_gateway_dispatch/);
});

// Hermes parses plugin.yaml with PyYAML and skips a plugin whose manifest fails to
// parse, so line checks alone miss a broken manifest. CI installs PyYAML, so this
// runs there either way; elsewhere it skips rather than fails when PyYAML is missing.
const hasPyYaml = spawnSync(pythonExe(), ['-c', 'import yaml'], { stdio: 'ignore', timeout: 30000 }).status === 0;

test('Hermes plugin manifest parses as YAML with the declared fields', { skip: !hasPyYaml && !process.env.CI && 'no Python with PyYAML' }, () => {
  const manifest = JSON.parse(python(String.raw`
import json, yaml
print(json.dumps(yaml.safe_load(open('plugin.yaml', encoding='utf-8'))))
`));
  const packageJson = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
  const skillDirs = fs.readdirSync(path.join(root, 'skills'))
    .filter((name) => fs.existsSync(path.join(root, 'skills', name, 'SKILL.md')))
    .sort();

  assert.equal(manifest.name, 'puritanical');
  assert.equal(manifest.version, packageJson.version);
  assert.equal(manifest.author, packageJson.author.name);
  assert.equal(typeof manifest.description, 'string');
  assert.deepEqual(manifest.provides_hooks, ['pre_llm_call', 'pre_gateway_dispatch']);
  assert.deepEqual(manifest.provides_commands, commands);
  assert.deepEqual([...manifest.provides_skills].sort(), skillDirs);
});

test('Hermes plugin registers every shipped skill under the puritanical namespace', () => {
  const output = python(String.raw`
import importlib.util, json, pathlib
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
class Ctx:
    def __init__(self):
        self.skills = []
        self.hooks = []
        self.commands = []
    def register_skill(self, name, path):
        self.skills.append((name, pathlib.Path(path).as_posix()))
    def register_hook(self, name, handler):
        self.hooks.append(name)
    def register_command(self, name, handler, description='', args_hint=''):
        self.commands.append(name)
ctx = Ctx()
mod.register(ctx)
print(json.dumps({'skills': ctx.skills, 'hooks': ctx.hooks, 'commands': ctx.commands}, sort_keys=True))
`);
  const data = JSON.parse(output);
  assert.deepEqual(data.skills.map(([name]) => name).sort(), [
    'puritanical',
    'puritanical-audit',
    'puritanical-debt',
    'puritanical-help',
    'puritanical-review',
  ]);
  assert.ok(data.skills.every(([, skillPath]) => skillPath.endsWith('/SKILL.md')));
  assert.ok(data.hooks.includes('pre_llm_call'));
  assert.ok(data.commands.includes('puritanical'));
  assert.ok(data.commands.includes('puritanical-review'));
});

test('Hermes plugin builds mode-aware injected context from the canonical skill', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-config-'));
  const output = python(String.raw`
import importlib.util, json
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
ctx = mod.build_injected_context('ultra')
print(json.dumps({'ctx': ctx}))
`, { XDG_CONFIG_HOME: tmp });
  const { ctx } = JSON.parse(output);

  assert.match(ctx, /PURITANICAL MODE ACTIVE — level: ultra/);
  assert.match(ctx, /Complete the requested work/);
  assert.match(ctx, /ultra/i);
  assert.doesNotMatch(ctx, /^---/);
  assert.doesNotMatch(ctx, /\|\s*\*\*Lite\*\*/i);
});

test('Hermes mode config respects env, config file, off, and invalid command behavior', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-config-'));
  fs.mkdirSync(path.join(tmp, 'puritanical'), { recursive: true });
  fs.writeFileSync(path.join(tmp, 'puritanical', 'config.json'), JSON.stringify({ defaultMode: 'lite' }));
  const output = python(String.raw`
import importlib.util, json
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
class Ctx:
    def __init__(self): self.commands = {}
    def register_skill(self, name, path): pass
    def register_hook(self, name, handler): pass
    def register_command(self, name, handler, description='', args_hint=''):
        self.commands[name] = handler
ctx = Ctx()
mod.register(ctx)
status_before = ctx.commands['puritanical']('')
invalid = ctx.commands['puritanical']('maximum')
status_after = ctx.commands['puritanical']('')
print(json.dumps({
    'default': mod.build_injected_context(None),
    'off': mod.build_injected_context('off'),
    'status_before': status_before,
    'invalid': invalid,
    'status_after': status_after,
}))
`, { XDG_CONFIG_HOME: tmp, PURITANICAL_DEFAULT_MODE: 'ultra' });
  const data = JSON.parse(output);
  assert.match(data.default, /level: ultra/);
  assert.equal(data.off, '');
  assert.match(data.status_before, /Puritanical mode: ultra/);
  assert.match(data.invalid, /Usage:/);
  assert.match(data.status_after, /Puritanical mode: ultra/);
});

test('Hermes honors plain and BOM-prefixed default-mode config before LLM calls', (t) => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-config-'));
  t.after(() => fs.rmSync(tmp, { recursive: true, force: true }));
  fs.mkdirSync(path.join(tmp, 'puritanical'));
  for (const prefix of ['', '\uFEFF']) {
    for (const mode of ['lite', 'off']) {
      fs.writeFileSync(path.join(tmp, 'puritanical', 'config.json'), prefix + JSON.stringify({ defaultMode: mode }));
      const output = python(String.raw`
import importlib.util, json
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
print(json.dumps(mod._pre_llm_call()))
`, { XDG_CONFIG_HOME: tmp, PURITANICAL_DEFAULT_MODE: '' });
      const data = JSON.parse(output);
      if (mode === 'off') assert.equal(data, null);
      else assert.match(data.context, /PURITANICAL MODE ACTIVE — level: lite/);
    }
  }
});

test('Hermes falls back to the default mode when the config file is unusable', (t) => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-config-'));
  t.after(() => fs.rmSync(tmp, { recursive: true, force: true }));
  fs.mkdirSync(path.join(tmp, 'puritanical'));
  const config = path.join(tmp, 'puritanical', 'config.json');
  const contents = {
    missing: null,
    malformed: '{ "defaultMode": ',
    'not UTF-8': Buffer.from([0xff, 0xfe, 0x00]),
    'deeply nested': '['.repeat(200000),
    'an array': '["lite"]',
    null: 'null',
    'an unknown level': '{ "defaultMode": "maximum" }',
  };
  for (const [name, content] of Object.entries(contents)) {
    fs.rmSync(config, { force: true });
    if (content !== null) fs.writeFileSync(config, content);
    const output = python(String.raw`
import importlib.util
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
print(mod._default_mode())
`, { XDG_CONFIG_HOME: tmp, PURITANICAL_DEFAULT_MODE: '' });
    assert.equal(output.trim(), 'full', name);
  }
});

test('Hermes treats review as a command, never a level', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-review-default-'));
  fs.mkdirSync(path.join(tmp, 'puritanical'), { recursive: true });
  fs.writeFileSync(path.join(tmp, 'puritanical', 'config.json'), JSON.stringify({ defaultMode: 'review' }));
  const output = python(String.raw`
import importlib.util, json, os
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
env_default = mod.build_injected_context(None)
del os.environ['PURITANICAL_DEFAULT_MODE']
file_default = mod.build_injected_context(None)
explicit_review = mod.build_injected_context('review')
print(json.dumps({
    'env_default': env_default,
    'file_default': file_default,
    'explicit_review': explicit_review,
}))
`, { XDG_CONFIG_HOME: tmp, PURITANICAL_DEFAULT_MODE: 'review' });
  const data = JSON.parse(output);
  assert.match(data.env_default, /PURITANICAL MODE ACTIVE — level: full/);
  assert.match(data.file_default, /PURITANICAL MODE ACTIVE — level: full/);
  assert.match(data.explicit_review, /PURITANICAL MODE ACTIVE — level: full/);
});

test('Hermes /puritanical command changes mode and pre_llm_call injects current context', () => {
  const output = python(String.raw`
import importlib.util, json
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
class Ctx:
    def __init__(self):
        self.hooks = {}
        self.commands = {}
    def register_skill(self, name, path): pass
    def register_hook(self, name, handler): self.hooks[name] = handler
    def register_command(self, name, handler, description='', args_hint=''):
        self.commands[name] = handler
ctx = Ctx()
mod.register(ctx)
message = ctx.commands['puritanical']('ultra')
injected = ctx.hooks['pre_llm_call'](session_id='s1', user_message='build it', conversation_history=[], is_first_turn=False, model='m', platform='cli')
print(json.dumps({'message': message, 'context': injected['context']}))
`);
  const data = JSON.parse(output);
  assert.match(data.message, /ultra/);
  assert.match(data.context, /PURITANICAL MODE ACTIVE — level: ultra/);
});

test('Hermes pre_llm_call does not append the same ruleset to every persisted turn', () => {
  const output = python(String.raw`
import importlib.util, json
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
first = mod._pre_llm_call(conversation_history=[])
history = [{
    'role': 'user',
    'content': 'first task',
    'api_content': 'first task\\n\\n' + first['context'],
}]
second = mod._pre_llm_call(conversation_history=history)
mod._handle_mode_command('ultra')
switched = mod._pre_llm_call(conversation_history=history)
history.append({
    'role': 'user',
    'content': 'second task',
    'api_content': 'second task\\n\\n' + switched['context'],
})
after_switch = mod._pre_llm_call(conversation_history=history)
print(json.dumps({'first': first, 'second': second, 'switched': switched, 'after_switch': after_switch}))
`);
  const data = JSON.parse(output);
  assert.match(data.first.context, /PURITANICAL MODE ACTIVE — level: full/);
  assert.equal(data.second, null);
  assert.match(data.switched.context, /PURITANICAL MODE ACTIVE — level: ultra/);
  assert.equal(data.after_switch, null);
});

test('Hermes /puritanical off reaches the model once, and a later level re-injects the ruleset', () => {
  // The ruleset from the first turn is replayed from api_content on every later
  // turn, and the command's reply goes to the user only, so off must be injected.
  const output = python(String.raw`
import importlib.util, json
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
first = mod._pre_llm_call(conversation_history=[])
history = [{
    'role': 'user',
    'content': 'first task',
    'api_content': 'first task\\n\\n' + first['context'],
}]
mod._handle_mode_command('off')
nothing_persisted = mod._pre_llm_call(conversation_history=[])
off = mod._pre_llm_call(conversation_history=history)
history.append({
    'role': 'user',
    'content': 'second task',
    'api_content': 'second task\\n\\nPURITANICAL MODE OFF',
})
still_off = mod._pre_llm_call(conversation_history=history)
mod._handle_mode_command('full')
back_on = mod._pre_llm_call(conversation_history=history)
print(json.dumps({'nothing_persisted': nothing_persisted, 'off': off, 'still_off': still_off, 'back_on': back_on}))
`);
  const data = JSON.parse(output);
  assert.equal(data.nothing_persisted, null);
  assert.deepEqual(data.off, { context: 'PURITANICAL MODE OFF' });
  assert.equal(data.still_off, null);
  assert.match(data.back_on.context, /PURITANICAL MODE ACTIVE — level: full/);
});

test('Hermes gateway rewrite respects slash access denial', () => {
  const output = python(String.raw`
import importlib.util, json
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
class Source:
    platform = None
    chat_id = 'c1'
    user_id = 'u1'
class Event:
    text = '/puritanical-review src/app.js'
    source = Source()
class Gateway:
    def _check_slash_access(self, source, command):
        return 'denied'
result = mod.rewrite_gateway_command(event=Event(), gateway=Gateway())
print(json.dumps(result))
`);
  assert.equal(output, 'null');
});

test('Hermes bare /puritanical switches an off session on and only reports a running level (#639)', () => {
  const output = python(String.raw`
import importlib.util, json
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
mod._set_mode('', 'ultra')
running = mod._handle_mode_command('')
kept = mod._session_mode('')
mod._set_mode('', 'off')
switched = mod._handle_mode_command('')
print(json.dumps([running, kept, switched, mod._session_mode('')]))
`);
  const [running, kept, switched, after] = JSON.parse(output);
  assert.match(running, /Puritanical mode: ultra/);
  assert.equal(kept, 'ultra');
  assert.match(switched, /set to full/);
  assert.equal(after, 'full');
});

// Hermes hands a command handler the calling session only when it declares session_id.
const hermesSessions = String.raw`
import importlib.util, inspect, json
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
def started(session_id):
    first = mod._pre_llm_call(session_id=session_id, user_message='first task', conversation_history=[])
    return [{'role': 'user', 'content': 'first task', 'api_content': 'first task\n\n' + first['context']}]
def turn(session_id, history, text='build it'):
    return mod._pre_llm_call(session_id=session_id, user_message=text, conversation_history=history)
def command(args, session_id):
    handler = mod._handle_mode_command
    if 'session_id' in inspect.signature(handler).parameters:
        return handler(args, session_id=session_id)
    return handler(args)
`;

test('Hermes keeps the level per session: one chat turning off leaves another on (R0808)', () => {
  const output = python(hermesSessions + String.raw`
a, b = started('a'), started('b')
reply = command('off', 'a')
print(json.dumps([reply, turn('a', a), turn('b', b)]))
`);
  const [reply, a, b] = JSON.parse(output);
  assert.match(reply, /set to off/);
  assert.deepEqual(a, { context: 'PURITANICAL MODE OFF' });
  assert.equal(b, null, 'session b keeps the ruleset it already has');
});

// Hermes passes session_id='' for a chat that has no session yet.
test('Hermes: a command from a chat with no session yet changes no other chat', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-config-'));
  const output = python(hermesSessions + String.raw`
b = started('b')
off = command('off', '')
after_off = turn('b', b)
default = command('default lite', '')
print(json.dumps([off, after_off, default, command('', 'c')]))
`, { XDG_CONFIG_HOME: tmp });
  const [off, b, saved, fresh] = JSON.parse(output);
  assert.match(off, /no session yet/);
  assert.equal(b, null, 'session b keeps its ruleset');
  assert.match(saved, /new sessions start in lite/);
  assert.match(fresh, /Puritanical mode: lite/, 'the default is saved and not pinned for every chat');
});

// PURITANICAL_DEFAULT_MODE outranks the saved default, so the reply must name the level
// new sessions will actually start in (R2954).
test('Hermes: /puritanical default names PURITANICAL_DEFAULT_MODE when it overrides the saved level', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-config-'));
  const output = python(hermesSessions + String.raw`
print(json.dumps([command('default lite', 'a'), command('default ultra', 'a')]))
`, { XDG_CONFIG_HOME: tmp, PURITANICAL_DEFAULT_MODE: 'ultra' });
  const [overridden, same] = JSON.parse(output);
  assert.equal(overridden, 'Puritanical default saved as lite, but PURITANICAL_DEFAULT_MODE is ultra, so new sessions start in ultra until it is unset.');
  assert.match(same, /^Puritanical default set: new sessions start in ultra/);
  assert.equal(JSON.parse(fs.readFileSync(path.join(tmp, 'puritanical', 'config.json'), 'utf8')).defaultMode, 'ultra');
});

test('Hermes switches a session off when its message is only "stop puritanical" or "normal mode" (R1973)', () => {
  const output = python(hermesSessions + String.raw`
print(json.dumps([turn(s, started(s), text) for s, text in [('a', 'Stop puritanical.'), ('b', 'normal mode'), ('c', 'add a normal mode toggle')]]))
`);
  const [a, b, c] = JSON.parse(output);
  assert.deepEqual(a, { context: 'PURITANICAL MODE OFF' });
  assert.deepEqual(b, { context: 'PURITANICAL MODE OFF' });
  assert.equal(c, null, 'a phrase inside a longer message changes nothing');
});

test('Hermes without session context for commands: a later /puritanical overrides an off phrase', () => {
  const output = python(hermesSessions + String.raw`
history = started('a')
turn('a', history, 'stop puritanical')
mod._handle_mode_command('full')
history.append({'role': 'user', 'content': 'x', 'api_content': 'x\n\nPURITANICAL MODE OFF'})
print(json.dumps(turn('a', history)))
`);
  assert.match(JSON.parse(output).context, /PURITANICAL MODE ACTIVE — level: full/);
});

test('Hermes without session context for commands: /puritanical default reaches new sessions', () => {
  const output = python(hermesSessions + String.raw`
saved = mod._handle_mode_command('default lite')
print(json.dumps([saved, turn('new', [])]))
`);
  const [saved, fresh] = JSON.parse(output);
  assert.match(saved, /new sessions start in lite/);
  assert.equal(fresh.context.split('\n')[0], 'PURITANICAL MODE ACTIVE — level: lite');
});

test('Hermes without session context for commands: a bare /puritanical resumes after an off phrase', () => {
  const output = python(hermesSessions + String.raw`
history = started('a')
off = turn('a', history, 'stop puritanical')
history.append({'role': 'user', 'content': 'x', 'api_content': 'x\n\nPURITANICAL MODE OFF'})
reply = mod._handle_mode_command('')
resumed = turn('a', history)
mod._handle_mode_command('default lite')
print(json.dumps([off, reply, resumed, turn('new', [])]))
`);
  const [off, reply, resumed, fresh] = JSON.parse(output);
  assert.deepEqual(off, { context: 'PURITANICAL MODE OFF' });
  assert.match(reply, /Puritanical mode: full/);
  assert.notEqual(resumed, null, 'the session that said "stop puritanical" gets the ruleset back');
  assert.equal(resumed.context.split('\n')[0], 'PURITANICAL MODE ACTIVE — level: full');
  assert.equal(fresh.context.split('\n')[0], 'PURITANICAL MODE ACTIVE — level: lite', 'resuming pins no level, so a later default still reaches new sessions');
});

test('Hermes /puritanical default saves the default, keeps other keys, and leaves the session level alone (R1973)', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-config-'));
  const config = path.join(tmp, 'puritanical', 'config.json');
  fs.mkdirSync(path.dirname(config), { recursive: true });
  fs.writeFileSync(config, JSON.stringify({ hideStatus: true }));
  const output = python(hermesSessions + String.raw`
saved = command('default lite', 'a')
print(json.dumps([saved, command('', 'a'), command('', 'b'), command('default review', 'a')]))
`, { XDG_CONFIG_HOME: tmp });
  const [saved, current, fresh, review] = JSON.parse(output);
  assert.match(saved, /new sessions start in lite/);
  assert.deepEqual(JSON.parse(fs.readFileSync(config, 'utf8')), { hideStatus: true, defaultMode: 'lite' });
  assert.match(current, /Puritanical mode: full/, 'the session that set the default keeps its level');
  assert.match(fresh, /Puritanical mode: lite/, 'a new session starts at the default');
  assert.match(review, /Usage:/);

  for (const unusable of ['{ not json', '[]']) {
    fs.writeFileSync(config, unusable);
    const kept = python(hermesSessions + String.raw`
print(json.dumps(command('default ultra', 'a')))
`, { XDG_CONFIG_HOME: tmp });
    assert.match(JSON.parse(kept), /not saved/);
    assert.equal(fs.readFileSync(config, 'utf8'), unusable, 'a config that is not a JSON object is left as it was');
  }
});

test('Hermes /puritanical default keeps the old config when the write or rename fails (R2420)', (t) => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-config-'));
  t.after(() => fs.rmSync(tmp, { recursive: true, force: true }));
  const dir = path.join(tmp, 'puritanical');
  const config = path.join(dir, 'config.json');
  fs.mkdirSync(dir);
  const original = JSON.stringify({ defaultMode: 'lite', hideStatus: true });
  // The full disk fails part way through the write, after the file is open, as a real one does.
  const failures = {
    'a full disk': String.raw`
import errno
real_fdopen = os.fdopen
class Full:
    def __init__(self, f): self.f = f
    def __enter__(self): return self
    def __exit__(self, *exc): self.f.close()
    def write(self, text):
        self.f.write(text[:11])
        self.f.flush()
        raise OSError(errno.ENOSPC, 'No space left on device')
mod.os.fdopen = lambda fd, *a, **k: Full(real_fdopen(fd, *a, **k))
`,
    'a failed rename': String.raw`
def busy(*a, **k): raise PermissionError('busy')
mod.os.replace = busy
`,
  };
  for (const [name, patch] of Object.entries(failures)) {
    fs.writeFileSync(config, original);
    const reply = python(hermesSessions + 'import os\n' + patch + String.raw`
print(json.dumps(command('default ultra', 'a')))
`, { XDG_CONFIG_HOME: tmp });
    assert.match(JSON.parse(reply), /not saved/, name);
    assert.equal(fs.readFileSync(config, 'utf8'), original, name);
    assert.deepEqual(fs.readdirSync(dir), ['config.json'], name);
  }

  fs.writeFileSync(config, original);
  fs.chmodSync(config, 0o444);
  const readOnly = python(hermesSessions + String.raw`
print(json.dumps(command('default ultra', 'a')))
`, { XDG_CONFIG_HOME: tmp });
  fs.chmodSync(config, 0o644);
  assert.match(JSON.parse(readOnly), /not saved/, 'a read-only config is refused');
  assert.equal(fs.readFileSync(config, 'utf8'), original, 'a read-only config is left as it was');
  assert.deepEqual(fs.readdirSync(dir), ['config.json']);

  if (process.platform !== 'win32') fs.chmodSync(config, 0o600);
  // The temp file is private from the start, not only once chmod runs.
  const [saved, tempModes] = JSON.parse(python(hermesSessions + String.raw`
import os, stat
os.umask(0o022)  # an inherited 077 would hide a temp file created 0644
real_chmod = os.chmod
seen = []
def chmod(p, m, *a, **k):
    seen.append(stat.S_IMODE(os.stat(p).st_mode))
    return real_chmod(p, m, *a, **k)
mod.os.chmod = chmod
print(json.dumps([command('default ultra', 'a'), seen]))
`, { XDG_CONFIG_HOME: tmp }));
  assert.match(saved, /new sessions start in ultra/);
  assert.deepEqual(JSON.parse(fs.readFileSync(config, 'utf8')), { defaultMode: 'ultra', hideStatus: true });
  if (process.platform !== 'win32') {
    assert.equal(fs.statSync(config).mode & 0o777, 0o600, 'the mode carries over');
    assert.deepEqual(tempModes, [0o600], 'the temp file is created with the old mode');
  }
  assert.deepEqual(fs.readdirSync(dir), ['config.json']);
});

test('Hermes /puritanical default refuses a symlinked config (R2420)', (t) => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'puritanical-config-'));
  t.after(() => fs.rmSync(tmp, { recursive: true, force: true }));
  const dir = path.join(tmp, 'puritanical');
  const config = path.join(dir, 'config.json');
  const victim = path.join(dir, 'victim.json');
  fs.mkdirSync(dir);
  fs.writeFileSync(victim, '{}');
  try { fs.symlinkSync(victim, config); } catch (e) {
    if (e.code !== 'EPERM') throw e;
    t.skip('symlink creation is not permitted');
    return;
  }
  const reply = python(hermesSessions + String.raw`
print(json.dumps(command('default ultra', 'a')))
`, { XDG_CONFIG_HOME: tmp });
  assert.match(JSON.parse(reply), /not saved/);
  assert.ok(fs.lstatSync(config).isSymbolicLink());
  assert.equal(fs.readFileSync(victim, 'utf8'), '{}');
  assert.deepEqual(fs.readdirSync(dir).sort(), ['config.json', 'victim.json']);

  // A link planted after the check, just before the rename, is replaced, not written through.
  fs.rmSync(config);
  const raced = python(hermesSessions + String.raw`
import os
real_replace = os.replace
def replace(src, dst):
    os.symlink(${JSON.stringify(victim)}, dst)
    return real_replace(src, dst)
mod.os.replace = replace
print(json.dumps(command('default ultra', 'a')))
`, { XDG_CONFIG_HOME: tmp });
  assert.match(JSON.parse(raced), /new sessions start in ultra/);
  assert.equal(fs.lstatSync(config).isSymbolicLink(), false);
  assert.equal(JSON.parse(fs.readFileSync(config, 'utf8')).defaultMode, 'ultra');
  assert.equal(fs.readFileSync(victim, 'utf8'), '{}');
});

test('Hermes gateway rewrite preserves every skill command and ignores unrelated text', () => {
  const output = python(String.raw`
import importlib.util, json
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
class Event:
    def __init__(self, text): self.text = text
cases = {}
for text in ['/puritanical-review x', '/puritanical_audit repo', '/puritanical-debt', '/puritanical-help', '/status', 'hello']:
    cases[text] = mod.rewrite_gateway_command(event=Event(text))
print(json.dumps(cases, sort_keys=True))
`);
  const data = JSON.parse(output);
  assert.match(data['/puritanical-review x'].text, /puritanical-review/);
  assert.match(data['/puritanical_audit repo'].text, /puritanical-audit/);
  assert.match(data['/puritanical_audit repo'].text, /repo/);
  assert.match(data['/puritanical-debt'].text, /puritanical-debt/);
  assert.match(data['/puritanical-help'].text, /puritanical-help/);
  assert.equal(data['/status'], null);
  assert.equal(data.hello, null);
});

test('Hermes mode filter does not drop a rule bullet whose label matches a mode name', () => {
  // Regression for the Hermes side of #571: a rule bullet shaped "- Full: ..."
  // has the same "label: text" shape as a worked example, but isn't one — it
  // must survive in every mode. Only the quoted `- lite: "..."`-style bullets
  // are real per-mode examples. hooks/puritanical-instructions.js already guards
  // against this; the Hermes plugin filter had drifted and lost the same guard.
  const output = python(String.raw`
import importlib.util, json
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
body = (
    "- Full: do not confuse this rule label with the mode name.\n"
    "- Lite: same risk, this is a real rule bullet.\n"
    '- lite: "real worked example"\n'
    '- ultra: "real worked example"'
)
print(json.dumps({'filtered': mod._filter_skill_body_for_mode(body, 'ultra')}))
`);
  const { filtered } = JSON.parse(output);
  assert.ok(filtered.includes('Full: do not confuse'), 'an unquoted rule bullet must not be treated as a mode example');
  assert.ok(filtered.includes('Lite: same risk'), 'an unquoted rule bullet must not be treated as a mode example');
  assert.ok(!filtered.includes('- lite:'), 'the real quoted lite example must still be filtered out in ultra mode');
  assert.ok(filtered.includes('ultra: "real worked example"'));
});

test('Hermes mode filter stays in lockstep with the JS filter on the real skill body', () => {
  // The two filters are independent ports of the same contract (one shared
  // skill file, every host adapter trims it the same way). Diff them directly
  // on skills/puritanical/SKILL.md instead of trusting the two implementations to
  // stay in sync by hand.
  const body = fs.readFileSync(path.join(root, 'skills', 'puritanical', 'SKILL.md'), 'utf8');
  for (const mode of ['lite', 'full', 'ultra']) {
    const jsFiltered = filterSkillBodyForMode(body, mode);
    const pyFiltered = python(String.raw`
import importlib.util, json
spec = importlib.util.spec_from_file_location('puritanical_hermes_plugin', '__init__.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
body = open('skills/puritanical/SKILL.md', encoding='utf-8').read()
print(json.dumps(mod._filter_skill_body_for_mode(body, '${mode}')))
`);
    assert.equal(JSON.parse(pyFiltered), jsFiltered, `filters diverge for mode "${mode}"`);
  }
});
