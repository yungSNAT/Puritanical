/* global require, module -- Node.js runtime */
'use strict';

// Installed beside ./chisle-hooks/; running from the repo, the core is in
// ../../../hooks/. Both loaders are literal requires.
function load(installed, repo) {
  try { return installed(); }
  catch (error) {
    if (error && error.code !== 'MODULE_NOT_FOUND') throw error;
    return repo();
  }
}

const core = load(() => require('./chisle-hooks/chisle-compress-output.js'),
  () => require('../../../hooks/chisle-compress-output.js'));
const config = load(() => require('./chisle-hooks/chisle-config.js'),
  () => require('../../../hooks/chisle-config.js'));
const CHISLE_MARK = '[chisle:';

module.exports = async function setup(ctx) {
  await ctx.tool.hook('execute.after', async (event) => {
    try {
      if (!event || event.status !== 'completed' || !event.result) return;
      const content = event.result.content;
      const compress = (text) => {
        if (typeof text !== 'string') return text;
        if (text.includes(CHISLE_MARK) && text.length <= core.limitsFor().maxChars) return text;
        const updated = core.compressForOpencode(event.tool, text, {
          mode: config.getDefaultMode(),
          sessionId: event.sessionID,
          callId: event.id,
          command: event.args && event.args.command,
        });
        if (updated == null || updated.length >= text.length) return text;
        core.recordSavings(text.length - updated.length, config.getOpencodeStateDir());
        return updated;
      };
      const updated = typeof content === 'string'
        ? compress(content)
        : Array.isArray(content)
          ? content.map((part) => part && part.type === 'text' ? { ...part, text: compress(part.text) } : part)
          : content;
      if (updated !== content) event.result = { ...event.result, content: updated };
    } catch {}
  });
};
