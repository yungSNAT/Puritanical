/* global module -- Node.js runtime */
'use strict';

// Shared mode-directive parser. Harness adapters decide how to persist mode.
// 'Puritanical' is an everyday word, so it switches the mode only in a whole
// instruction: the slash command, 'puritanical [mode] on|off', 'puritanical
// mode', or 'activate'/'stop puritanical [mode]' standing alone.
const OFF_ARGS = new Set(['off', 'stop', 'disable', 'deactivate']);

function requestedMode(prompt) {
  const text = String(prompt || '').trim().toLowerCase();
  if (/\bnormal mode\b/.test(text)) return 'off';
  const legacy = legacyMode(text);
  if (legacy) return legacy;

  const slash = /^\/puritanical(?::puritanical)?\b(?:\s+(\S+))?/.exec(text);
  if (slash) return OFF_ARGS.has(slash[1]) ? 'off' : 'on';
  const order = text.replace(/[.!]+$/, '').replace(/^please\s+/, '');
  const named = /^puritanical(\s+mode)?(?:\s+(on|off))?$/.exec(order);
  if (named && (named[1] || named[2])) return named[2] === 'off' ? 'off' : 'on';
  if (/^(?:activate|enable|turn on|start)\s+puritanical(?:\s+mode)?$/.test(order)) return 'on';
  if (/^(?:stop|disable|deactivate|turn off|exit|kill)\s+puritanical(?:\s+mode)?$/.test(order)) return 'off';
  return null;
}

// Chisle's phrases work as they always did: that name is no everyday word (formerly Chisle).
function legacyMode(text) {
  if (/\b(?:turn off|disable|deactivate|stop|kill|exit)\s+chisle\b|\bchisle\s+(?:mode\s+(?:off|stop|disable|deactivate)|off|stop|disable|deactivate)\b/.test(text)) return 'off'; // formerly Chisle
  if (/^\/chisle(?::chisle)?\b|\b(?:activate|enable|turn on|start|use)\b.*\bchisle\b|\bchisle\b.*\b(?:mode|activate|enable|on)\b|\bchislif(?:y|ier)\b/.test(text)) return 'on'; // formerly Chisle
  return null;
}

module.exports = { requestedMode };
