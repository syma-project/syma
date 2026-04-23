# Sym: A Symbolic-First Language with OOP Structure

## 1. Philosophy

Sym is a dynamically typed, symbolic-first programming language that combines the
expressiveness of Wolfram Language with the organizational power of object-oriented
programming.

**Design principles:**

- **Everything is a symbolic expression.** The AST is the data. Code is data you can inspect,
  transform, and generate at runtime.
- **Pattern matching is universal.** Function dispatch, control flow, destructuring, and
  rewrite rules all use the same pattern language.
- **OOP is structural sugar.** Classes, inheritance, and modules compile down to symbolic
  expressions and rule sets — they organize code, not constrain it.
- **JIT-compiled.** Start as a tree-walk interpreter, compile hot paths to native code at
  runtime. Pattern matchers become specialized DFA automata.

---

## 2. Syntax Overview

### 2.1 Expressions

Everything in Sym is an expression of the form `head[arg1, arg2, ...]`:

```
f[x, y]              (* function application *)
Sin[Pi/2]            (* built-in function call *)
Plus[1, 2, 3]        (* same as 1 + 2 + 3 *)
{1, 2, 3}            (* List[1, 2, 3] — list literal *)
```

Operators are syntactic sugar for expression heads:

| Syntax | Meaning |
|--------|---------|
| `a + b` | `Plus[a, b]` |
| `a * b` | `Times[a, b]` |
| `a^b` | `Power[a, b]` |
| `a == b` | `Equal[a, b]` |
| `a != b` | `Unequal[a, b]` |
| `a -> b` | `Rule[a, b]` (immediate) |
| `a :> b` | `RuleDelayed[a, b]` (lazy) |
| `a /. rules` | `ReplaceAll[a, rules]` |
| `a //. rules` | `ReplaceRepeated[a, rules]` |
| `f @ x` | `f[x]` (prefix application) |
| `x // f` | `f[x]` (postfix application) |

### 2.2 Atoms

```
42                  (* Integer *)
3.14                (* Real *)
Rational[2, 3]      (* Rational — use Rational[] constructor *)
1 + 2I              (* Complex *)
"hello"             (* String *)
True, False         (* Boolean *)
Pi, E, I            (* numeric constants *)
x, myVar            (* Symbol — unevaluated unless defined *)
```

### 2.3 Comments

```
(* single-line comment *)
(* multi-line
   comment *)
```

### 2.4 Semicolons and Sequences

```
a; b; c              (* Sequence — evaluates all, returns last *)
expr;                (* suppress output (returns Null) *)
```

---

## 3. Assignment and Definitions

### 3.1 Immediate vs Delayed

```
x = 5                (* immediate: evaluates RHS now, stores result *)
f[x_] := x^2        (* delayed: evaluates RHS each time f is called *)
```

`=` evaluates the right-hand side once. `:=` re-evaluates on every call — essential for
symbolic computation where you want fresh pattern matches.

### 3.2 Function Definitions

Functions are defined by pattern matching on arguments:

```
f[x_] := x^2                    (* single pattern *)
f[x_Integer] := x^2             (* type-constrained *)
f[x_, y_] := x + y              (* multiple arguments *)
f[x_, x_] := 2x                 (* repeated pattern — both args equal *)
f[0] := 1                        (* literal pattern *)
f[n_Integer /; n > 0] := n * f[n-1]   (* guard condition *)
```

Multiple definitions for the same function coexist. The evaluator tries them in order,
most-specific-first:

```
factorial[0] := 1
factorial[n_Integer /; n > 0] := n * factorial[n - 1]
```

### 3.3 Pure Functions (Lambda)

```
(#^2 &)                    (* lambda: x -> x^2 *)
(#1 + #2 &)                (* lambda: (x, y) -> x + y *)
Function[x, x^2]           (* named parameter form *)
Function[{x, y}, x + y]    (* multiple parameters *)
```

---

## 4. Type System

Sym is dynamically typed. Types are runtime values, not compile-time annotations.
Type checking happens through pattern matching at call time.

### 4.1 Built-in Type Hierarchy

```
Expr                          (* root — everything is an Expr *)
├── Atom
│   ├── Integer               (* 42, -7, 0 *)
│   ├── Real                  (* 3.14, -0.5 *)
│   ├── Rational              (* 2/3 *)
│   ├── Complex               (* 1 + 2I *)
│   ├── String                (* "hello" *)
│   ├── Symbol                (* x, Pi, True *)
│   └── Boolean               (* True, False *)
├── Compound
│   ├── List                  (* {1, 2, 3} *)
│   ├── Rule                  (* a -> b *)
│   ├── RuleDelayed           (* a :> b *)
│   └── Pattern               (* _Integer, x_ *)
├── Function                  (* compiled/callable *)
└── Object                    (* class instance *)
```

### 4.2 Type Patterns

Type patterns constrain which values a pattern variable can match:

```
x_                  (* matches anything, binds to x *)
x_Integer           (* matches only Integers *)
x_Number            (* matches Integer, Real, Rational, Complex *)
x_String            (* matches only Strings *)
_                   (* matches anything, discards (blank) *)
__                  (* matches one or more (sequence) *)
___                 (* matches zero or more (optional sequence) *)
```

### 4.3 Type Predicates

```
x.is[Integer]           (* True if x is an Integer *)
MatchQ[x, _Number]      (* True if x matches the Number pattern *)
Head[x]                 (* returns the type head: Integer, List, etc. *)
TypeOf[x]               (* same as Head, but for objects returns class name *)
```

### 4.4 Type Annotations (Optional)

Type annotations are comments to the reader and to tooling — they are not enforced
by the runtime:

```
f[x_: Integer, y_: String] := ...    (* annotation, not a constraint *)
```

The actual dispatch is still handled by pattern definitions:

```
f[x_Integer, y_String] := ...        (* this is the enforced version *)
```

---

## 5. Pattern Matching

Pattern matching is the core dispatch mechanism. It appears in function definitions,
`match` expressions, rule application, and destructuring.

### 5.1 Pattern Syntax

```
_                   (* Blank: matches any single expression *)
x_                  (* Named blank: matches and binds to x *)
_Integer            (* Typed blank: matches only Integers *)
x_Integer           (* Named typed blank *)
__                  (* BlankSequence: matches 1+ expressions *)
x__                 (* named *)
___                 (* BlankNullSequence: matches 0+ expressions *)
{x_, y_}            (* list destructuring *)
{a_, b__, c_}       (* a = first, b = middle sequence, c = last *)
```

### 5.2 Guards

Guards add boolean conditions to patterns:

```
x_Integer /; x > 0              (* positive integers only *)
{x_, y_} /; x < y              (* ordered pair *)
```

### 5.3 Pattern Alternatives

```
(x_Integer | x_Real)            (* matches Integer or Real *)
f[(0 | 1)] := "binary"         (* matches 0 or 1 *)
```

### 5.4 Match Expressions

`match` is the general-purpose pattern dispatch block:

```
match expr {
    x_Integer /; x > 0  => "positive integer: " <> ToString[x]
    x_Integer            => "non-positive integer"
    x_String             => "string: " <> x
    _                    => "something else"
}
```

The `=>` separates pattern from result. The first matching branch wins.

### 5.5 Destructuring Assignment

```
{a, b, c} = {1, 2, 3}          (* a=1, b=2, c=3 *)
{x_, y_} = point               (* destructure a list-valued symbol *)
{head, tail__} = myList         (* head = first, tail = rest *)
```

### 5.6 Rewrite Rules

Rules are first-class values that map patterns to replacements:

```
rule trigSimplify = {
    Sin[x_]^2 + Cos[x_]^2  -> 1
    Sin[0]                  -> 0
    Cos[0]                  -> 1
}

rule algebraic = {
    0 * _          -> 0
    1 * x_         -> x
    x_ + 0         -> x
    x_ ^ 0         -> 1
    x_ ^ 1         -> x
}
```

Apply rules with `/.` (once) or `//.` (repeatedly until stable):

```
Sin[x]^2 + Cos[x]^2 /. trigSimplify        (* => 1 *)
expr //. {algebraic, trigSimplify}          (* apply both rule sets *)
```

---

## 6. Classes and Objects

Classes provide encapsulation, inheritance, and polymorphism. Under the hood, a class
is a symbolic expression constructor paired with a rule set for method dispatch.

### 6.1 Class Definition

```
class Polynomial {
    field coeffs: List           (* field declaration *)
    field var: Symbol = 'x       (* field with default value *)

    constructor[coeffs_List, var_: 'x] {
        this.coeffs = coeffs
        this.var = var
    }

    method evaluate[x_] := Sum[
        coeffs * x^Range[0, Length[coeffs] - 1]
    ]

    method derivative[] := Polynomial[
        Drop[coeffs * Range[0, Length[coeffs] - 1], 1],
        var
    ]

    method degree[] := Length[coeffs] - 1

    method toString[] := StringJoin[
        Riffle[
            MapIndexed[
                #1 * var^#2[[1]] &,
                coeffs
            ],
            " + "
        ]
    ]
}
```

### 6.2 Object Construction

```
p = Polynomial[{3, 2, 1}]

p.coeffs            (* => {3, 2, 1} *)
p.var               (* => x *)
p.evaluate[2]       (* => 3 + 2*2 + 1*4 = 11 *)
p.derivative[]      (* => Polynomial[{2, 2}, x] *)
p.degree[]          (* => 2 *)
```

### 6.3 Pattern Dispatch on Object Methods

Methods support multiple definitions with pattern matching — just like standalone
functions:

```
class Shape {
    method area[] := "unknown"

    method describe[] := "Shape with area " <> ToString[this.area[]]
}

class Circle extends Shape {
    field radius

    constructor[r_] { this.radius = r }

    method area[] := Pi * radius^2

    method scale[f_] := Circle[radius * f]
}

class Rectangle extends Shape {
    field width, height

    constructor[w_, h_] {
        this.width = w
        this.height = h
    }

    method area[] := width * height

    method isSquare[] := width == height
}
```

Usage:

```
c = Circle[5]
c.area[]            (* => 25 Pi *)
c.describe[]        (* => "Shape with area 25 Pi" *)
c.scale[2].area[]   (* => 100 Pi *)

r = Rectangle[3, 4]
r.area[]            (* => 12 *)
r.isSquare[]        (* => False *)
```

### 6.4 Inheritance and Mixins

```
class Child extends Parent { ... }         (* single inheritance *)
class Child extends Parent with Mixin1, Mixin2 { ... }  (* mixins *)
```

A mixin is a class without a constructor — it only contributes methods:

```
mixin Printable {
    method print[] := Print[this.toString[]]
    method debug[] := Print[Head[this], ": ", this.toString[]]
}

class Polynomial with Printable { ... }    (* now has print[] and debug[] *)
```

### 6.5 Special Methods

```
class Vector {
    field components: List

    (* operator overloading via special method names *)
    method __add__[other_Vector] := Vector[components + other.components]
    method __mul__[scalar_] := Vector[components * scalar]
    method __eq__[other_Vector] := components == other.components
    method __len__[] := Length[components]
    method __getitem__[i_] := components[[i]]
    method __repr__[] := "Vector[" <> ToString[components] <> "]"
}
```

Now `v1 + v2` calls `v1.__add__[v2]`, and `v * 3` calls `v.__mul__[3]`.

### 6.6 Class as Pattern

A class name acts as a type pattern in pattern matching:

```
area[c_Circle] := Pi * c.radius^2
area[r_Rectangle] := r.width * r.height
area[s_] := s.area[]              (* fallback: call method *)

match shape {
    c_Circle    => "circle r=" <> ToString[c.radius]
    r_Rectangle => "rectangle " <> ToString[r.width] <> "x" <> ToString[r.height]
    _           => "unknown shape"
}
```

### 6.7 Objects as Symbolic Expressions

An object is internally represented as:

```
Object[ClassName, {field1 -> val1, field2 -> val2, ...}]
```

This means objects participate in pattern matching and rule application just like
any other expression:

```
rule expandPoly = {
    Polynomial[cs_, v_] /; Length[cs] > 2 :>
        Polynomial[Take[cs, 2], v] + Polynomial[Drop[cs, 2], v] * v^2
}
```

---

## 7. Rules and Transformations

Rules are the bridge between symbolic computation and OOP. They operate on
expressions — including objects.

### 7.1 Rule Definition

```
rule name = { pattern -> replacement, ... }       (* immediate *)
rule name = { pattern :> replacement, ... }       (* delayed *)
```

### 7.2 Rule Application

```
expr /. rule            (* apply rule once — first match *)
expr //. rule           (* apply repeatedly until no more matches *)
expr /. {r1, r2, r3}   (* apply a rule set *)
```

### 7.3 Conditional Rules

```
rule simplify = {
    x_ + x_          -> 2x
    x_ * x_          -> x^2
    Power[Power[x_, a_], b_] -> x^(a * b)
    Log[Power[x_, n_]]      -> n * Log[x] /; x > 0
}
```

### 7.4 Class-Attached Transformations

Classes can define transformation rules that auto-apply:

```
class Tensor {
    field data: List
    field dims: List

    @transform normalize {
        Tensor[d_, dims_] /; Max[Abs[d]] > 0 :>
            Tensor[d / Max[Abs[d]], dims]
    }

    @transform flatten {
        Tensor[d_, _] :> Tensor[Flatten[d], {Length[Flatten[d]]}]
    }
}
```

`@transform` methods are automatically applied during evaluation when the
object appears in an expression:

```
t = Tensor[{{1, 2}, {3, 4}}, {2, 2}]
t.normalize[]       (* explicitly call *)
```

---

## 8. Modules

Modules provide namespace isolation and explicit export control.

### 8.1 Module Definition

```
module LinearAlgebra {
    export Matrix, determinant, eigenvalues, Vector

    class Matrix {
        field data: List
        field rows: Integer
        field cols: Integer

        constructor[data_] {
            this.data = data
            this.rows = Length[data]
            this.cols = Length[data[[1]]]
        }

        method __mul__[other_Matrix] := Matrix[
            Table[
                Sum[this.data[[i, k]] * other.data[[k, j]], {k, this.cols}],
                {i, this.rows}, {j, other.cols}
            ]
        ]

        method transpose[] := Matrix[Transpose[data]]
        method det[] := determinant[this]
    }

    class Vector {
        field components: List
        constructor[cs_] { this.components = cs }
        method norm[] := Sqrt[components . components]
    }

    determinant[m_Matrix] := Det[m.data]

    eigenvalues[m_Matrix] := Eigenvalues[m.data]
}
```

### 8.2 Module Import

```
import LinearAlgebra                    (* import everything exported *)
import LinearAlgebra.{Matrix, Vector}   (* selective import *)
import LinearAlgebra as LA              (* alias *)
```

### 8.3 Modules as First-Class Values

Modules are values — they can be passed to functions, returned, and composed:

```
withBackend[module_] := {
    import module.{Matrix}
    Matrix[{{1, 0}, {0, 1}}]
}
```

---

## 9. Control Flow

All control flow constructs are expressions that return values.

### 9.1 Conditionals

```
If[condition, then, else]

Which[
    x > 0,  "positive",
    x < 0,  "negative",
    True,   "zero"
]

Switch[expr,
    _Integer, "integer",
    _String,  "string",
    _,        "other"
]
```

### 9.2 Loops

```
For[i = 1, i <= 10, i++, body]

While[condition, body]

Do[body, {i, 1, 10}]           (* iterate i from 1 to 10 *)
Do[body, {i, list}]             (* iterate over list elements *)
```

### 9.3 Functional Iteration (Preferred)

```
Map[f, {1, 2, 3}]              (* => {f[1], f[2], f[3]} *)
f /@ {1, 2, 3}                 (* same — infix Map *)

Fold[f, init, {1, 2, 3}]      (* => f[f[f[init, 1], 2], 3] *)

Select[{1, 2, 3, 4}, EvenQ]   (* => {2, 4} *)

Scan[f, {1, 2, 3}]            (* like Map but returns Null *)

Nest[f, x, 3]                  (* => f[f[f[x]]] *)

FixedPoint[f, x]               (* apply f until result stabilizes *)
```

### 9.4 Exception Handling

```
try {
    riskyOperation[]
} catch err {
    match err {
        e_SymError  => "Sym error: " <> e.message
        e_TypeError => "Type error: " <> e.message
        _           => "Unknown error"
    }
} finally {
    cleanup[]
}

throw SymError["something went wrong"]
```

---

## 10. Built-in Symbolic Functions

Sym ships with a core symbolic library:

### 10.1 Mathematics

```
Sin, Cos, Tan, ArcSin, ArcCos, ArcTan    (* trigonometric *)
Log, Exp, Sqrt, Abs                       (* transcendental *)
Plus, Times, Power, Divide                (* arithmetic (also operators) *)
D[expr, x]                                (* symbolic derivative *)
Integrate[expr, x]                        (* symbolic integral *)
Simplify[expr]                            (* apply built-in simplification *)
Expand[expr]                              (* expand products *)
Factor[expr]                              (* factor polynomials *)
Solve[equation, x]                        (* symbolic equation solving *)
Series[expr, {x, x0, n}]                 (* Taylor series *)
```

### 10.2 Lists

```
Length[list]                    (* element count *)
First[list], Last[list]         (* access ends *)
Rest[list]                      (* all but first *)
Most[list]                      (* all but last *)
Append[list, elem]              (* add to end *)
Prepend[list, elem]             (* add to beginning *)
Join[list1, list2]              (* concatenate *)
Flatten[list]                   (* remove nesting *)
Sort[list]                      (* sort *)
Reverse[list]                   (* reverse *)
Take[list, n], Drop[list, n]   (* sublists *)
Part[list, i] or list[[i]]     (* index access *)
```

### 10.3 Strings

```
StringJoin[s1, s2, ...] or s1 <> s2    (* concatenate *)
StringLength[s]                         (* character count *)
StringSplit[s, delim]                   (* split *)
StringReplace[s, rule]                  (* replace *)
ToString[expr]                          (* expression to string *)
ToExpression[s]                         (* string to expression *)
```

### 10.4 Association (Hash Map)

```
assoc = <|"a" -> 1, "b" -> 2, "c" -> 3|>
assoc["a"]                     (* => 1 *)
assoc["d"] = 4                 (* add key *)
Keys[assoc]                    (* => {"a", "b", "c", "d"} *)
Values[assoc]                  (* => {1, 2, 3, 4} *)
```

---

## 11. Evaluation Semantics

### 11.1 Evaluation Order

1. **Parse** source into symbolic expressions.
2. **Evaluate head** — resolve the function/rule to call.
3. **Evaluate arguments** — left to right (unless `Hold` attributes prevent it).
4. **Pattern match** — find the best-matching definition.
5. **Evaluate body** — substitute bound variables, evaluate result.
6. **Apply rules** — if the result has `@transform` rules, apply them.

### 11.2 Hold and Evaluation Control

```
Hold[1 + 2]                   (* => Hold[1 + 2] — prevents evaluation *)
HoldComplete[expr]             (* prevents even inner evaluation *)
ReleaseHold[Hold[1 + 2]]      (* => 3 *)

SetAttributes[f, HoldAll]     (* f does not evaluate its arguments *)
SetAttributes[f, HoldFirst]   (* f holds only the first argument *)
SetAttributes[f, HoldRest]    (* f holds all but the first argument *)
```

### 11.3 Attributes

Attributes modify how a function's arguments are treated:

```
Listable           (* auto-thread over lists: f[{1,2}] => {f[1], f[2]} *)
Flat               (* associative: f[f[a,b], c] => f[a,b,c] *)
Orderless          (* commutative: f[b,a] => f[a,b] *)
OneIdentity        (* f[x] => x when f has one argument *)
Protected          (* cannot be redefined *)
```

---

## 12. JIT Execution Model

Sym uses a three-phase JIT compilation strategy:

### Phase 1: Tree-Walk Interpreter

- Expressions are evaluated directly as AST nodes.
- Pattern matching walks the tree structurally.
- Fast startup, no compilation overhead.
- Used for: REPL, scripts, cold code paths.

### Phase 2: Bytecode Compiler

- Frequently-executed functions are compiled to Sym bytecode.
- A virtual machine executes bytecode with a register-based architecture.
- Pattern matchers are compiled to decision trees.
- Used for: warm code paths, loops, recursive functions.

### Phase 3: Native JIT

- Hot bytecode is compiled to native code via Cranelift or LLVM.
- **Pattern specialization:** after a pattern fires N times, the JIT generates
  a specialized matcher for the observed value shapes. If `f[x_Integer]` is
  always called with small integers, the JIT compiles a fast integer-only path
  with a fallback to the general matcher.
- **Expression freezing:** symbolic expressions that stop changing form (reached
  a fixed point under rule application) are "frozen" — their tree structure is
  linearized into native memory for cache-friendly access.
- **Inline caching:** method calls on objects cache the class of the receiver.
  Monomorphic call sites (always the same class) compile to direct jumps.
- Used for: hot loops, numerical computation, large-scale symbolic transformation.

### JIT Compilation Triggers

```
HotFunctionThreshold = 100      (* compile after 100 calls *)
HotLoopThreshold = 50           (* compile after 50 loop iterations *)
PatternSpecializeThreshold = 10 (* specialize pattern after 10 matches *)
```

---

## 13. Interoperability

### 13.1 FFI (Foreign Function Interface)

```
extern "C" {
    function fastMul(a: Pointer, b: Pointer): Pointer
    function initArith(): Void
}

(* Call native C functions from Sym *)
result = fastMul[ptr1, ptr2]
```

### 13.2 Serialization

```
Export["data.json", expr]            (* to JSON *)
Import["data.json"]                  (* from JSON *)
Export["data.sym", expr]             (* native binary format *)
Import["data.sym"]
```

### 13.3 System Commands

```
RunProcess["ls", {"-la"}]           (* run shell command *)
FileRead["path/to/file"]            (* read file *)
FileWrite["path/to/file", content]  (* write file *)
```

---

## 14. Sample Programs

### 14.1 Symbolic Differentiation

```
(* Derivative rules — the heart of symbolic computation *)
rule derivative = {
    D[c_, x_] /; FreeQ[c, x]   -> 0            (* constant *)
    D[x_, x_]                   -> 1            (* identity *)
    D[x_^n_, x_]                -> n * x^(n-1)  (* power *)
    D[u_ + v_, x_]              -> D[u, x] + D[v, x]  (* sum *)
    D[u_ * v_, x_]              -> D[u, x]*v + u*D[v, x]  (* product *)
    D[Sin[u_], x_]              -> Cos[u] * D[u, x]
    D[Cos[u_], x_]              -> -Sin[u] * D[u, x]
    D[Exp[u_], x_]              -> Exp[u] * D[u, x]
    D[Log[u_], x_]              -> D[u, x] / u
    D[f_[u_], x_]               -> D[u, x] * Derivative[1][f][u]  (* chain rule *)
}

D[Sin[x]^2, x] //. derivative
(* => 2 Sin[x] Cos[x] *)

D[x^3 + 2x^2 + x + 1, x] //. derivative
(* => 3x^2 + 4x + 1 *)
```

### 14.2 Polynomial Arithmetic with OOP

```
class Polynomial {
    field coeffs: List
    field var: Symbol = 'x

    constructor[coeffs_, var_: 'x] {
        this.coeffs = coeffs
        this.var = var
    }

    method evaluate[x_] := Sum[
        coeffs[[i+1]] * x^i,
        {i, 0, Length[coeffs] - 1}
    ]

    method derivative[] := Polynomial[
        Table[i * coeffs[[i+1]], {i, 1, Length[coeffs] - 1}],
        var
    ]

    method __add__[other_Polynomial] := Polynomial[
        PadRight[coeffs, Max[Length[coeffs], Length[other.coeffs]]]
        + PadRight[other.coeffs, Max[Length[coeffs], Length[other.coeffs]]],
        var
    ]

    method __mul__[other_Polynomial] := Polynomial[
        ListConvolve[coeffs, other.coeffs],
        var
    ]

    method toString[] := StringJoin @@ Riffle[
        Reverse @ Select[
            MapIndexed[
                If[#1 != 0,
                    ToString[#1] <> If[#2[[1]] > 1, var^ToString[#2[[1]]-1],
                        If[#2[[1]] == 1, ToString[var], ""]], ""]
                &,
                coeffs
            ],
            # != "" &
        ],
        " + "
    ]
}

(* Usage *)
p = Polynomial[{1, 2, 3}]          (* 1 + 2x + 3x^2 *)
q = Polynomial[{0, 1}]             (* x *)

p.evaluate[2]                       (* => 17 *)
p.derivative[]                      (* => Polynomial[{2, 6}] => 2 + 6x *)
(p * q).toString[]                  (* => "x + 2x^2 + 3x^3" *)
```

### 14.3 Physics Simulation: Gravitational N-Body

```
module NBody {
    export Body, simulate, GravitationalSystem

    class Body {
        field mass: Real
        field pos: List          (* {x, y, z} *)
        field vel: List          (* {vx, vy, vz} *)

        constructor[mass_, pos_, vel_: {0, 0, 0}] {
            this.mass = mass
            this.pos = pos
            this.vel = vel
        }

        method kineticEnergy[] := 0.5 * mass * vel . vel

        method momentum[] := mass * vel
    }

    class GravitationalSystem {
        field bodies: List
        field G: Real = 6.674e-11

        constructor[bodies_] { this.bodies = bodies }

        method forceOn[i_Integer] := Sum[
            If[j != i,
                G * bodies[[i]].mass * bodies[[j]].mass /
                    Norm[bodies[[j]].pos - bodies[[i]].pos]^3 *
                    (bodies[[j]].pos - bodies[[i]].pos),
                {0, 0, 0}
            ],
            {j, Length[bodies]}
        ]

        method step[dt_] := {
            forces = Map[this.forceOn, Range[Length[bodies]]]

            this.bodies = Table[
                Body[
                    bodies[[i]].mass,
                    bodies[[i]].pos + bodies[[i]].vel * dt,
                    bodies[[i]].vel + forces[[i]] / bodies[[i]].mass * dt
                ],
                {i, Length[bodies]}
            ]
        }

        method simulate[dt_, steps_] := Do[
            this.step[dt],
            {steps}
        ]

        method totalEnergy[] := Sum[
            bodies[[i]].kineticEnergy[] +
            Sum[
                If[j > i,
                    -G * bodies[[i]].mass * bodies[[j]].mass /
                        Norm[bodies[[j]].pos - bodies[[i]].pos],
                    0
                ],
                {j, Length[bodies]}
            ],
            {i, Length[bodies]}
        ]
    }
}

(* Usage *)
import NBody

sun = Body[1.989e30, {0, 0, 0}]
earth = Body[5.972e24, {1.496e11, 0, 0}, {0, 29783, 0}]

system = GravitationalSystem[{sun, earth}]
system.simulate[3600, 24]     (* 24 hours in 1-hour steps *)
Print[earth.pos]              (* new position after 1 day *)
Print[system.totalEnergy[]]   (* should be approximately conserved *)
```

### 14.4 Symbolic Pattern-Based Simplifier

```
module Simplifier {
    export simplify, fullSimplify

    rule arithmetic = {
        x_ + 0         -> x
        0 + x_         -> x
        x_ * 1         -> x
        1 * x_         -> x
        x_ * 0         -> 0
        0 * x_         -> 0
        x_ ^ 0         -> 1
        x_ ^ 1         -> x
        x_ / 1         -> x
    }

    rule algebraic = {
        x_ + x_        -> 2 * x
        x_ * x_        -> x^2
        x_ - x_        -> 0
    }

    rule trigonometric = {
        Sin[x_]^2 + Cos[x_]^2  -> 1
        Sin[-x_]                -> -Sin[x]
        Cos[-x_]                -> Cos[x]
        Tan[x_]                 -> Sin[x] / Cos[x]
    }

    rule logarithmic = {
        Log[1]          -> 0
        Log[E]          -> 1
        Log[x_ * y_]    -> Log[x] + Log[y]
        Log[x_^n_]      -> n * Log[x]
        Exp[Log[x_]]    -> x
        Log[Exp[x_]]    -> x
    }

    allRules = {arithmetic, algebraic, trigonometric, logarithmic}

    simplify[expr_] := expr //. allRules

    fullSimplify[expr_] := FixedPoint[
        # //. allRules &,
        expr
    ]
}

(* Usage *)
import Simplifier

simplify[2*x + 3*x]             (* => 5x *)
simplify[Sin[x]^2 + Cos[x]^2 + 1]  (* => 2 *)
simplify[Log[E^3]]              (* => 3 *)
simplify[x * x * x]             (* => x^3 *)
```

---

## 15. Grammar Summary (EBNF)

Notation: `|` = alternation, `{ }` = repetition (0+), `[ ]` = optional, `( )` = grouping.
Labels like `{...}R` indicate right-associative repetition.

Lexer notes:
- `.` after a digit (e.g. `3.14`) is a decimal point (real literal). `.` after a non-digit is member access.
- `/`, `/.`, `//.`, `//`, `/@` are distinct tokens. Maximal munch: `//.` before `//` before `/`.
- `@`, `@@` are distinct tokens. Maximal munch: `@@` before `@`.
- `:=` is a single token (not `:` + `=`). `:>` is a single token.
- `->` is a single token. `=>` is a single token.
- `[[` and `]]` are single tokens (double bracket), distinct from `[` and `]`.

```
(* ── Top-level ── *)

program        = { statement ";" }

statement      = definition | expression | import_stmt | export_stmt

definition     = func_def | rule_def | class_def | module_def | mixin_def
                 | assign

func_def       = ident "[" [ pattern { "," pattern } ] "]" ":=" expression
rule_def       = "rule" ident "=" "{" { rule_line } "}"
class_def      = "class" ident [ "extends" ident ] [ "with" ident_list ]
                 "{" { member_def } "}"
mixin_def      = "mixin" ident "{" { member_def } "}"
module_def     = "module" ident "{" { statement } "}"

assign         = assignable "=" expression

assignable     = ident
                 | assignable "." ident
                 | assignable "[" expr_list "]"
                 | assignable "[[" expr_list "]]"
                 | "{" destruct_pattern { "," destruct_pattern } "}"
                 (* destructuring: binds variables on LHS *)

destruct_pattern = ident [ "_" [ type_suffix ] ]
                   | "_" [ type_suffix ]
                   | "__" | "___"

import_stmt    = "import" qualified_ident
                 [ "." "{" ident_list "}" ]
                 [ "as" ident ]

export_stmt    = "export" ident_list

qualified_ident = ident { "." ident }

(* ── Expressions (precedence low → high) ── *)

expression     = pipe_expr

pipe_expr      = at_expr { "//" at_expr }
                 (* left-associative postfix: a // f // g = g[f[a]] *)

at_expr        = rule_expr { ("@" | "@@") at_expr }R
                 (* @ = Prefix (f @ x = f[x]), @@ = Apply — both right-associative *)

rule_expr      = or_expr { ("->" | ":>") or_expr }R
                 (* right-associative: a -> b -> c => a -> (b -> c) *)

or_expr        = and_expr { "||" and_expr }
and_expr       = comp_expr { "&&" comp_expr }
comp_expr      = add_expr { ("==" | "!=" | "<" | ">" | "<=" | ">=") add_expr }
add_expr       = mul_expr { ("+" | "-") mul_expr }
mul_expr       = pow_expr { ("*" | "/" | "/." | "//." | "/@") pow_expr }
                 (* /. = ReplaceAll, //. = ReplaceRepeated, /@ = Map *)
pow_expr       = unary_expr { "^" unary_expr }R
                 (* right-associative: 2^3^2 = 2^(3^2) = 512 *)

unary_expr     = ("-" | "!" | "'" | "~") unary_expr | postfix_expr

postfix_expr   = primary_expr {
                     "." ident                              (* member access *)
                   | "." ident "[" [ expr_list ] "]"        (* method call *)
                   | "[" [ expr_list ] "]"                  (* function/builtin call *)
                   | "[[" expr_list "]]"                    (* part/index access *)
                 }

primary_expr   = atom
                 | "(" expression ")"
                 | "{" [ expr_list ] "}"                    (* list literal *)
                 | "<|" [ assoc_entries ] "|>"              (* association *)
                 | "match" expression "{" { match_branch } "}"
                 | "If" "[" expression "," expression [ "," expression ] "]"
                 | "Which" "[" expr_pair { "," expr_pair } "]"
                 | "Switch" "[" expression "," expr_pair { "," expr_pair } "]"
                 | "try" "{" { statement ";" } "}"
                   "catch" ident "{" { statement ";" } "}"
                   [ "finally" "{" { statement ";" } "}" ]
                 | "For" "[" expression "," expression "," expression "," expression "]"
                 | "While" "[" expression "," expression "]"
                 | "Do" "[" expression "," iterator_spec "]"

(* ── Atoms ── *)

atom           = integer | real | complex_lit
                 | string | symbol
                 | "True" | "False" | "Null"
                 | "#" | "#" integer                        (* slot *)
                 | "Function" "[" params "," expression "]"
                 | "Hold" "[" expression "]"
                 | "HoldComplete" "[" expression "]"

(* Rationals: 2/3 is parsed as Divide[2, 3], not a rational literal.
   Use Rational[2, 3] for an exact rational value. *)

complex_lit    = integer "I" | real "I"
                 | integer "+" integer "I" | integer "-" integer "I"

(* ── Helpers ── *)

expr_list      = expression { "," expression }
expr_pair      = expression "," expression
ident_list     = ident { "," ident }
assoc_entries  = assoc_entry { "," assoc_entry }
assoc_entry    = ( string | ident ) "->" expression
params         = ident | "{" ident { "," ident } "}"
iterator_spec  = "{" ident "," expression "}"
                 | "{" ident "," expression "," expression "}"

rule_line      = pattern ("->" | ":>") expression

member_def     = field_def | method_def | constructor_def | transform_def

field_def      = "field" ident [ ":" type_name ] [ "=" expression ]
method_def     = "method" ident "[" [ pattern { "," pattern } ] "]"
                 [ ":" type_name ] ( ":=" expression | "{" { statement ";" } "}" )
constructor_def = "constructor" "[" [ pattern { "," pattern } ] "]"
                  "{" { statement ";" } "}"
transform_def  = "@transform" ident "{" { rule_line } "}"

type_name      = ident [ "[" type_name { "," type_name } "]" ]

(* ── Patterns ── *)

(* Patterns mirror expression syntax but replace leaf atoms with blanks.
   This supports compound patterns like -Sin[u_], x_^n_, a_ + b_. *)

pattern        = pat_pipe_expr [ "/;" expression ]

pat_pipe_expr  = pat_at_expr { "//" pat_at_expr }
pat_at_expr    = pat_rule_expr { ("@" | "@@") pat_at_expr }R
pat_rule_expr  = pat_or_expr { ("->" | ":>") pat_or_expr }R
pat_or_expr    = pat_and_expr { "||" pat_and_expr }
pat_and_expr   = pat_comp_expr { "&&" pat_comp_expr }
pat_comp_expr  = pat_add_expr { ("==" | "!=" | "<" | ">" | "<=" | ">=") pat_add_expr }
pat_add_expr   = pat_mul_expr { ("+" | "-") pat_mul_expr }
pat_mul_expr   = pat_pow_expr { ("*" | "/" | "/." | "//." | "/@") pat_pow_expr }
pat_pow_expr   = pat_unary_expr { "^" pat_unary_expr }R
pat_unary_expr = ("-" | "!" | "'" | "~") pat_unary_expr | pat_postfix_expr

pat_postfix_expr = pat_primary {
                     "." ident
                   | "." ident "[" [ pat_list ] "]"
                   | "[" [ pat_list ] "]"
                   | "[[" pat_list "]]"
                 }

pat_primary    = blank
                 | literal
                 | symbol
                 | "(" pat_alt ")"
                 | "{" [ pat_list ] "}"
                 | "match" expression "{" { match_branch } "}"

blank          = [ ident ] "_" [ type_suffix ]
                 (* _ = anonymous, x_ = named, _Integer = typed, x_Integer = named+typed *)
                 | [ ident ] "__" [ type_suffix ]           (* sequence: 1+ *)
                 | [ ident ] "___" [ type_suffix ]          (* optional sequence: 0+ *)

literal        = integer | real | string | "True" | "False" | "Null"

type_suffix    = ident                                      (* e.g. Integer, Number, String *)

pat_list       = pattern { "," pattern }
pat_alt        = pattern { "|" pattern }

match_branch   = pattern "=>" expression ";"
```

---

## 16. Comparison with Wolfram Language

| Feature | Wolfram Language | Sym |
|---------|-----------------|-----|
| Typing | Dynamic | Dynamic |
| Dispatch | Pattern-based | Pattern-based |
| Encapsulation | None (contexts only) | Classes with fields/methods |
| Inheritance | None | `extends` + `with` (mixins) |
| Operator overloading | Limited | `__add__`, `__mul__`, etc. |
| Modules | Packages (files) | First-class `module` blocks |
| Rules | First-class | First-class, can attach to classes |
| Pattern syntax | `_Integer`, `x_` | `_Integer`, `x_` (same) |
| Evaluation | `/.`, `//.` | `/.`, `//.` (same) |
| Execution | Interpreted | JIT (3-phase) |
| Homoiconicity | Full | Full |
| Comments | `(* *)` | `(* *)` |

---

## 17. Naming Convention

- **Built-in functions:** PascalCase — `Sin`, `Map`, `Length`, `Solve`
- **User functions:** camelCase — `myFunc`, `computeArea`
- **Classes:** PascalCase — `Polynomial`, `Matrix`, `Circle`
- **Constants:** UPPER_SNAKE — `MAX_ITERATIONS`, `DEFAULT_TOLERANCE`
- **Module names:** PascalCase — `LinearAlgebra`, `Simplifier`
- **Pattern variables:** camelCase — `x_`, `coeffs_`, `other_Polynomial`
