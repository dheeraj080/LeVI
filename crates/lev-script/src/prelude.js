// SPDX-License-Identifier: MIT OR Apache-2.0
// The `lev.*` API, built in JavaScript over a handful of native primitives.
//
// Keeping the ergonomic surface in JS (and the sandbox primitives in Rust) means the
// API can grow — matchers, helpers, eventually a `pm.*` compatibility prelude — without
// touching the Rust bindings. The natives are all `__lev.<fn>` and deal in JSON strings
// and primitives only, which is what keeps the FFI surface tiny and auditable.
//
// This file is evaluated once per script invocation, after the natives are installed.
(function () {
  var n = __lev;

  function repr(value) {
    try {
      return JSON.stringify(value);
    } catch (e) {
      return String(value);
    }
  }

  // A chai/k6-style `expect`. Matchers throw on failure; `lev.test` catches the throw
  // and records a failed check, so `expect` inside a test reads naturally.
  function expect(actual) {
    function fail(message) {
      throw new Error(message);
    }
    return {
      toBe: function (expected) {
        if (actual !== expected) fail('expected ' + repr(actual) + ' to be ' + repr(expected));
      },
      toEqual: function (expected) {
        if (JSON.stringify(actual) !== JSON.stringify(expected)) {
          fail('expected ' + repr(actual) + ' to equal ' + repr(expected));
        }
      },
      toBeTruthy: function () {
        if (!actual) fail('expected ' + repr(actual) + ' to be truthy');
      },
      toBeFalsy: function () {
        if (actual) fail('expected ' + repr(actual) + ' to be falsy');
      },
      toContain: function (needle) {
        if (String(actual).indexOf(String(needle)) === -1) {
          fail('expected ' + repr(actual) + ' to contain ' + repr(needle));
        }
      },
      toBeGreaterThan: function (n2) {
        if (!(actual > n2)) fail('expected ' + repr(actual) + ' > ' + repr(n2));
      },
      toBeGreaterThanOrEqual: function (n2) {
        if (!(actual >= n2)) fail('expected ' + repr(actual) + ' >= ' + repr(n2));
      },
      toBeLessThan: function (n2) {
        if (!(actual < n2)) fail('expected ' + repr(actual) + ' < ' + repr(n2));
      },
      toBeLessThanOrEqual: function (n2) {
        if (!(actual <= n2)) fail('expected ' + repr(actual) + ' <= ' + repr(n2));
      },
      toHaveLength: function (expected) {
        if (!actual || actual.length !== expected) {
          fail('expected length ' + expected + ', got ' + repr(actual));
        }
      },
      toMatch: function (pattern) {
        if (!new RegExp(pattern).test(String(actual))) {
          fail('expected ' + repr(actual) + ' to match ' + pattern);
        }
      }
    };
  }

  // `lev.test(name, fnOrBool)` — the one way checks are recorded.
  function test(name, value) {
    var passed = false;
    var message = null;
    try {
      if (typeof value === 'function') {
        value();
        passed = true;
      } else {
        passed = !!value;
      }
    } catch (e) {
      passed = false;
      message = e && e.message ? String(e.message) : String(e);
    }
    n.emitCheck(String(name), passed, message);
    return passed;
  }

  var random = {
    int: function (min, max) {
      return n.randInt(min, max);
    },
    float: function () {
      return n.randFloat();
    },
    uuid: function () {
      return n.uuid();
    },
    string: function (len) {
      return n.randString(len);
    },
    pick: function (array) {
      return array[n.randInt(0, array.length - 1)];
    }
  };

  var env = {
    get: function (name) {
      var value = n.envGet(String(name));
      return value === null ? undefined : value;
    },
    set: function (name, value) {
      n.envSet(String(name), String(value));
    },
    has: function (name) {
      return n.envGet(String(name)) !== null;
    },
    all: function () {
      return JSON.parse(n.varsJson());
    }
  };

  var request = n.requestJson() ? JSON.parse(n.requestJson()) : null;
  var response = n.responseJson() ? JSON.parse(n.responseJson()) : null;
  if (response && typeof response.body === 'string' && response.body.length > 0) {
    try {
      response.json = JSON.parse(response.body);
    } catch (e) {
      // Not JSON; leave `json` undefined rather than throwing at load time.
    }
  }

  globalThis.lev = {
    test: test,
    expect: expect,
    random: random,
    env: env,
    log: function (message) {
      n.log(String(message));
    },
    request: request,
    response: response
  };
})();
