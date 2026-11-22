/* global module -- Node.js runtime */
'use strict';

// Shared mode-directive parser. Harness adapters decide how to persist mode.
function requestedMode(prompt) {
  const text = String(prompt || '').trim().toLowerCase();

  if (/\b(turn off|disable|deactivate|stop|kill|exit)\s+puritanical\b/i.test(text) ||
      /\bpuritanical\s+(?:mode\s+(?:off|stop|disable|deactivate)|off|stop|disable|deactivate)\b/i.test(text) ||
      /\bnormal mode\b/i.test(text)) return 'off';

  if (/^\/puritanical(\b|:puritanical\b)/.test(text) ||
      /\b(activate|enable|turn on|start|use)\b.*\bpuritanical\b/i.test(text) ||
      /\bpuritanical\b.*\b(mode|activate|enable|on)\b/i.test(text) ||
      /\bpuritanif(y|ier)\b/i.test(text)) return 'on';

  return null;
}

module.exports = { requestedMode };
