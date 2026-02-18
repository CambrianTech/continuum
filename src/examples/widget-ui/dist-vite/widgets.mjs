var N = Object.defineProperty;
var e = (t, i) => N(t, "name", { value: i, configurable: !0 });
var U = /* @__PURE__ */ Symbol.for("preact-signals");
function p() {
  if (v > 1)
    v--;
  else {
    for (var t, i = !1; a !== void 0; ) {
      var o = a;
      for (a = void 0, l++; o !== void 0; ) {
        var r = o.o;
        if (o.o = void 0, o.f &= -3, !(8 & o.f) && g(o)) try {
          o.c();
        } catch (f) {
          i || (t = f, i = !0);
        }
        o = r;
      }
    }
    if (l = 0, v--, i) throw t;
  }
}
e(p, "t");
var n = void 0;
function w(t) {
  var i = n;
  n = void 0;
  try {
    return t();
  } finally {
    n = i;
  }
}
e(w, "n");
var a = void 0, v = 0, l = 0, d = 0;
function S(t) {
  if (n !== void 0) {
    var i = t.n;
    if (i === void 0 || i.t !== n)
      return i = { i: 0, S: t, p: n.s, n: void 0, t: n, e: void 0, x: void 0, r: i }, n.s !== void 0 && (n.s.n = i), n.s = i, t.n = i, 32 & n.f && t.S(i), i;
    if (i.i === -1)
      return i.i = 0, i.n !== void 0 && (i.n.p = i.p, i.p !== void 0 && (i.p.n = i.n), i.p = n.s, i.n = void 0, n.s.n = i, n.s = i), i;
  }
}
e(S, "e");
function s(t, i) {
  this.v = t, this.i = 0, this.n = void 0, this.t = void 0, this.W = i?.watched, this.Z = i?.unwatched, this.name = i?.name;
}
e(s, "u");
s.prototype.brand = U;
s.prototype.h = function() {
  return !0;
};
s.prototype.S = function(t) {
  var i = this, o = this.t;
  o !== t && t.e === void 0 && (t.x = o, this.t = t, o !== void 0 ? o.e = t : w(function() {
    var r;
    (r = i.W) == null || r.call(i);
  }));
};
s.prototype.U = function(t) {
  var i = this;
  if (this.t !== void 0) {
    var o = t.e, r = t.x;
    o !== void 0 && (o.x = r, t.e = void 0), r !== void 0 && (r.e = o, t.x = void 0), t === this.t && (this.t = r, r === void 0 && w(function() {
      var f;
      (f = i.Z) == null || f.call(i);
    }));
  }
};
s.prototype.subscribe = function(t) {
  var i = this;
  return c(function() {
    var o = i.value, r = n;
    n = void 0;
    try {
      t(o);
    } finally {
      n = r;
    }
  }, { name: "sub" });
};
s.prototype.valueOf = function() {
  return this.value;
};
s.prototype.toString = function() {
  return this.value + "";
};
s.prototype.toJSON = function() {
  return this.value;
};
s.prototype.peek = function() {
  var t = n;
  n = void 0;
  try {
    return this.value;
  } finally {
    n = t;
  }
};
Object.defineProperty(s.prototype, "value", { get: /* @__PURE__ */ e(function() {
  var t = S(this);
  return t !== void 0 && (t.i = this.i), this.v;
}, "get"), set: /* @__PURE__ */ e(function(t) {
  if (t !== this.v) {
    if (l > 100) throw new Error("Cycle detected");
    this.v = t, this.i++, d++, v++;
    try {
      for (var i = this.t; i !== void 0; i = i.x) i.t.N();
    } finally {
      p();
    }
  }
}, "set") });
function C(t, i) {
  return new s(t, i);
}
e(C, "d");
function g(t) {
  for (var i = t.s; i !== void 0; i = i.n) if (i.S.i !== i.i || !i.S.h() || i.S.i !== i.i) return !0;
  return !1;
}
e(g, "c");
function x(t) {
  for (var i = t.s; i !== void 0; i = i.n) {
    var o = i.S.n;
    if (o !== void 0 && (i.r = o), i.S.n = i, i.i = -1, i.n === void 0) {
      t.s = i;
      break;
    }
  }
}
e(x, "a");
function b(t) {
  for (var i = t.s, o = void 0; i !== void 0; ) {
    var r = i.p;
    i.i === -1 ? (i.S.U(i), r !== void 0 && (r.n = i.n), i.n !== void 0 && (i.n.p = r)) : o = i, i.S.n = i.r, i.r !== void 0 && (i.r = void 0), i = r;
  }
  t.s = o;
}
e(b, "l");
function h(t, i) {
  s.call(this, void 0), this.x = t, this.s = void 0, this.g = d - 1, this.f = 4, this.W = i?.watched, this.Z = i?.unwatched, this.name = i?.name;
}
e(h, "y");
h.prototype = new s();
h.prototype.h = function() {
  if (this.f &= -3, 1 & this.f) return !1;
  if ((36 & this.f) == 32 || (this.f &= -5, this.g === d)) return !0;
  if (this.g = d, this.f |= 1, this.i > 0 && !g(this))
    return this.f &= -2, !0;
  var t = n;
  try {
    x(this), n = this;
    var i = this.x();
    (16 & this.f || this.v !== i || this.i === 0) && (this.v = i, this.f &= -17, this.i++);
  } catch (o) {
    this.v = o, this.f |= 16, this.i++;
  }
  return n = t, b(this), this.f &= -2, !0;
};
h.prototype.S = function(t) {
  if (this.t === void 0) {
    this.f |= 36;
    for (var i = this.s; i !== void 0; i = i.n) i.S.S(i);
  }
  s.prototype.S.call(this, t);
};
h.prototype.U = function(t) {
  if (this.t !== void 0 && (s.prototype.U.call(this, t), this.t === void 0)) {
    this.f &= -33;
    for (var i = this.s; i !== void 0; i = i.n) i.S.U(i);
  }
};
h.prototype.N = function() {
  if (!(2 & this.f)) {
    this.f |= 6;
    for (var t = this.t; t !== void 0; t = t.x) t.t.N();
  }
};
Object.defineProperty(h.prototype, "value", { get: /* @__PURE__ */ e(function() {
  if (1 & this.f) throw new Error("Cycle detected");
  var t = S(this);
  if (this.h(), t !== void 0 && (t.i = this.i), 16 & this.f) throw this.v;
  return this.v;
}, "get") });
function W(t, i) {
  return new h(t, i);
}
e(W, "w");
function m(t) {
  var i = t.u;
  if (t.u = void 0, typeof i == "function") {
    v++;
    var o = n;
    n = void 0;
    try {
      i();
    } catch (r) {
      throw t.f &= -2, t.f |= 8, y(t), r;
    } finally {
      n = o, p();
    }
  }
}
e(m, "_");
function y(t) {
  for (var i = t.s; i !== void 0; i = i.n) i.S.U(i);
  t.x = void 0, t.s = void 0, m(t);
}
e(y, "b");
function E(t) {
  if (n !== this) throw new Error("Out-of-order effect");
  b(this), n = t, this.f &= -2, 8 & this.f && y(this), p();
}
e(E, "g");
function u(t, i) {
  this.x = t, this.u = void 0, this.s = void 0, this.o = void 0, this.f = 32, this.name = i?.name;
}
e(u, "p");
u.prototype.c = function() {
  var t = this.S();
  try {
    if (8 & this.f || this.x === void 0) return;
    var i = this.x();
    typeof i == "function" && (this.u = i);
  } finally {
    t();
  }
};
u.prototype.S = function() {
  if (1 & this.f) throw new Error("Cycle detected");
  this.f |= 1, this.f &= -9, m(this), x(this), v++;
  var t = n;
  return n = this, E.bind(this, t);
};
u.prototype.N = function() {
  2 & this.f || (this.f |= 2, this.o = a, a = this);
};
u.prototype.d = function() {
  this.f |= 8, 1 & this.f || y(this);
};
u.prototype.dispose = function() {
  this.d();
};
function c(t, i) {
  var o = new u(t, i);
  try {
    o.c();
  } catch (f) {
    throw o.d(), f;
  }
  var r = o.d.bind(o);
  return r[Symbol.dispose] = r, r;
}
e(c, "E");
function O(t) {
  const i = {};
  for (const o in t)
    i[o] = C(t[o]);
  return {
    // Get current values
    get state() {
      const o = {};
      for (const r in i)
        o[r] = i[r].value;
      return o;
    },
    // Update a single property
    set(o, r) {
      i[o].value = r;
    },
    // Subscribe to changes (returns cleanup function)
    subscribe(o) {
      return c(() => {
        o(this.state);
      });
    },
    // Get raw signal for fine-grained reactivity
    getSignal(o) {
      return i[o];
    }
  };
}
e(O, "createWidgetStore");
const Z = /* @__PURE__ */ e(() => O({
  currentRoomId: null,
  roomName: "General",
  messages: [],
  isLoading: !1
}), "createRoomStore");
function j(t, i, o) {
  return c(() => {
    const f = o ? o(i.value) : String(i.value);
    t.textContent = f;
  });
}
e(j, "bindText");
function L(t, i, o) {
  return c(() => {
    t.classList.toggle(i, o.value);
  });
}
e(L, "bindClass");
console.log("🚀 Vite reactive primitives loaded");
export {
  L as bindClass,
  j as bindText,
  W as computed,
  Z as createRoomStore,
  O as createWidgetStore,
  c as effect,
  C as signal
};
