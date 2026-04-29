// ── Help documentation ──

/// Look up help text for a built-in symbol.
///
/// Returns the usage string (including attributes) if documented.
pub fn get_help(name: &str) -> Option<&'static str> {
    Some(match name {
        // ── Arithmetic ──
        "Plus" => "Plus[a, b, ...] or a + b + ... computes the sum of its arguments.",
        "Times" => "Times[a, b, ...] or a * b * ... computes the product of its arguments.",
        "Power" => "Power[a, b] or a ^ b gives a raised to the power b.",
        "Divide" => "Divide[a, b] or a / b gives a divided by b.",
        "Minus" => "Minus[x] or -x gives the negation of x.",
        "Abs" => "Abs[x] gives the absolute value of x.",

        // ── Comparison ──
        "Equal" => "Equal[a, b] or a == b returns True if a and b are equal.",
        "Unequal" => "Unequal[a, b] or a != b returns True if a and b are not equal.",
        "Less" => "Less[a, b] or a < b returns True if a is strictly less than b.",
        "Greater" => "Greater[a, b] or a > b returns True if a is strictly greater than b.",
        "LessEqual" => "LessEqual[a, b] or a <= b returns True if a is less than or equal to b.",
        "GreaterEqual" => {
            "GreaterEqual[a, b] or a >= b returns True if a is greater than or equal to b."
        }

        // ── Logical ──
        "And" => {
            "And[a, b, ...] or a && b && ... evaluates arguments left to right, returning \
             the first value that is False, or the last value if all are True.\n\
             And[] = True."
        }
        "Or" => {
            "Or[a, b, ...] or a || b || ... evaluates arguments left to right, returning \
             the first value that is True, or the last value if none are True.\n\
             Or[] = False."
        }
        "Not" => "Not[expr] or !expr returns the logical negation of expr.",
        "Xor" => {
            "Xor[a, b, ...] returns True if an odd number of arguments are True.\n\
             Xor[] = False."
        }
        "Nand" => {
            "Nand[a, b, ...] returns False if all arguments are True, True otherwise.\n\
             Nand[] = False."
        }
        "Nor" => {
            "Nor[a, b, ...] returns True if no argument is True, False otherwise.\n\
             Nor[] = True."
        }
        "Implies" => "Implies[p, q] returns True unless p is True and q is False (p → q).",
        "Equivalent" => {
            "Equivalent[a, b, ...] returns True if all arguments have the same truth value.\n\
             Equivalent[] = True."
        }
        "Majority" => {
            "Majority[a, b, c, ...] returns True if more than half of the arguments are True.\n\
             Requires an odd number of arguments."
        }
        "Boole" => "Boole[expr] returns 1 if expr is True, 0 otherwise.",
        "BooleanQ" => "BooleanQ[expr] returns True if expr is True or False, False otherwise.",

        // ── List ──
        "Length" => "Length[expr] gives the number of elements in expr.",
        "First" => "First[expr] gives the first element of expr.",
        "Last" => "Last[expr] gives the last element of expr.",
        "Rest" => "Rest[expr] gives expr with the first element removed.",
        "Most" => "Most[expr] gives expr with the last element removed.",
        "Append" => "Append[expr, elem] returns expr with elem appended.",
        "Prepend" => "Prepend[expr, elem] returns expr with elem prepended.",
        "Join" => "Join[list1, list2, ...] concatenates lists together.",
        "Flatten" => "Flatten[expr] flattens nested lists into a single list.",
        "Sort" => "Sort[list] sorts the elements of list into canonical order.",
        "Reverse" => "Reverse[expr] reverses the order of elements in expr.",
        "Part" => "Part[expr, i] or expr[[i]] gives the i-th part of expr.",
        "Range" => {
            "Range[n] gives {1, 2, ..., n}.\nRange[min, max] gives {min, min+1, ..., max}.\nRange[min, max, step] uses the given step."
        }
        "Table" => {
            "Table[expr, n] generates a list of n copies of expr.\nTable[expr, {i, max}] evaluates expr for i from 1 to max.\nTable[expr, {i, min, max}] evaluates expr for i from min to max.\nTable[expr, {i, min, max, step}] uses the given step.\nTable[expr, {i, {val1, val2, ...}}] uses successive values from the list.\nTable[expr, {i, imin, imax}, {j, jmin, jmax}, ...] gives a nested list."
        }
        "Map" => "Map[f, expr] or f /@ expr applies f to each element at level 1 of expr.",
        "Fold" => {
            "Fold[f, x, list] or Fold[f, list] folds a function from the left.\nFold[f, x, list] starts with initial value x.\nFold[f, list] uses the first element of list as the initial value."
        }
        "Select" => "Select[list, crit] picks elements of list for which crit returns True.",
        "Scan" => "Scan[f, expr] evaluates f applied to each element of expr, returning Null.",
        "Nest" => "Nest[f, expr, n] applies f to expr n times.",
        "Take" => {
            "Take[list, n] gives the first n elements of list.\nTake[list, -n] gives the last n elements.\nTake[list, {m, n}] gives elements m through n (inclusive)."
        }
        "Drop" => {
            "Drop[list, n] gives list with the first n elements removed.\nDrop[list, -n] removes the last n elements.\nDrop[list, {m, n}] removes elements m through n (inclusive)."
        }
        "Riffle" => "Riffle[list, x] inserts x between consecutive elements of list.",
        "Transpose" => "Transpose[list] transposes the first two levels of list.",
        "Total" => "Total[list] gives the total of all elements in list.",
        "Sum" => "Sum[expr, {i, min, max}] evaluates the sum of expr as i goes from min to max.",
        "Product" => {
            "Product[expr, {i, min, max}] evaluates a product of expr as i goes from min to max.\n\
             Product[expr, {i, max}] evaluates a product of expr for i from 1 to max."
        }
        "Partition" => {
            "Partition[list, n] splits list into sublists of length n.\nPartition[list, n, d] uses offset d between successive sublists."
        }
        "Split" => "Split[list] splits list into runs of identical adjacent elements.",
        "Gather" => "Gather[list] groups identical elements into sublists.",
        "DeleteDuplicates" => {
            "DeleteDuplicates[list] deletes all duplicates from list, keeping the first occurrence."
        }
        "Insert" => {
            "Insert[list, elem, n] inserts elem at position n in list (1-indexed, negative counts from end)."
        }
        "Delete" => {
            "Delete[list, n] deletes the element at position n in list (1-indexed, negative counts from end)."
        }
        "ReplacePart" => {
            "ReplacePart[list, n, new] replaces the element at position n in list with new."
        }
        "RotateLeft" => "RotateLeft[list, n] rotates the elements of list n positions to the left.",
        "RotateRight" => {
            "RotateRight[list, n] rotates the elements of list n positions to the right."
        }
        "Ordering" => {
            "Ordering[list] returns the positions that would sort list.\nOrdering[list, n] returns the first n positions.\nOrdering[list, -n] returns the last n positions."
        }
        "ConstantArray" => "ConstantArray[val, n] creates a list of n copies of val.",
        "Diagonal" => "Diagonal[matrix] extracts the diagonal elements from a matrix.",
        "Accumulate" => {
            "Accumulate[list] computes the running total (cumulative sum) of list elements."
        }
        "Differences" => "Differences[list] computes the adjacent differences of list elements.",
        "Clip" => {
            "Clip[x] clamps x to the range [-1, 1].\nClip[x, {min, max}] clamps x to the range [min, max]."
        }
        "Chop" => {
            "Chop[expr] replaces approximate real numbers close to 0 with exact 0.\nChop[expr, tol] uses tolerance tol (default 1e-10)."
        }
        "Unitize" => "Unitize[x] returns 0 if x == 0, 1 otherwise.",
        "Ramp" => "Ramp[x] returns max(0, x).",
        "RealAbs" => "RealAbs[x] returns the absolute value of a real number x.",
        "RealSign" => "RealSign[x] returns -1, 0, or 1 for real x.",
        "LogisticSigmoid" => "LogisticSigmoid[x] returns 1/(1+exp(-x)).",
        "NumericalOrder" => {
            "NumericalOrder[x, y] returns -1 if x < y, 0 if x == y, 1 if x > y (numeric comparison)."
        }
        "UnitBox" => "UnitBox[x] returns 1 if |x| < 1/2, 1/2 if |x| == 1/2, 0 otherwise.",
        "UnitTriangle" => "UnitTriangle[x] returns max(0, 1-|x|).",
        "Array" => {
            "Array[f, n] generates {f[1], f[2], ..., f[n]}.\nArray[f, {n}] generates {f[1], f[2], ..., f[n]}.\nArray[f, {n, m}] generates {f[n], f[n+1], ..., f[m]}."
        }
        "SplitBy" => {
            "SplitBy[list, f] splits list into runs where f applied to each element gives identical values."
        }
        "GatherBy" => {
            "GatherBy[list, f] groups elements by the values of f applied to each element."
        }
        "FoldList" => {
            "FoldList[f, init, list] gives all intermediate results of folding f from the left."
        }
        "NestList" => {
            "NestList[f, expr, n] gives all intermediate results of applying f to expr n times."
        }
        "MapApply" => {
            "MapApply[f, expr] (f @@@ expr) replaces heads at level 1, using elements of lists as arguments.\n\
             MapApply[f, {{a,b}, {c,d}}] → {f[a,b], f[c,d]}."
        }
        "MovingAverage" => {
            "MovingAverage[list, n] computes the moving average of list with window size n."
        }
        "BlockMap" => {
            "BlockMap[f, list, n] partitions list into non-overlapping blocks of size n and applies f to each."
        }
        "ListConvolve" => {
            "ListConvolve[kernel, list] computes the convolution of kernel with list.\n\
             ListConvolve[{k1,k2}, {a,b,c}] → {a*k1+b*k2, b*k1+c*k2}."
        }
        "Nearest" => {
            "Nearest[list, x] returns the element in list closest to x.\n\
             Nearest[list, x, n] returns the n closest elements."
        }
        "ArrayPad" => {
            "ArrayPad[list, n] pads list with n zeros on each side.\n\
             ArrayPad[list, {before, after}] pads with different amounts on each side.\n\
             ArrayPad[list, n, val] pads with val instead of 0."
        }
        "ArrayReshape" => {
            "ArrayReshape[list, {d1, d2, ...}] reshapes a flat list into the given dimensions.\n\
             The total number of elements must match the product of dimensions.\n\
             Example: ArrayReshape[{1,2,3,4,5,6}, {2,3}] → {{1,2,3},{4,5,6}}."
        }
        "StringCases" => {
            "StringCases[string, pattern] finds all substrings matching the pattern.\n\
             Supports literal string matching and \"*\" as a wildcard.\n\
             Example: StringCases[\"abcabc\", \"ab\"] → {\"ab\", \"ab\"}."
        }

        // ── Pattern ──
        "MatchQ" => "MatchQ[expr, pattern] returns True if expr matches pattern.",
        "Head" => "Head[expr] gives the head of expr (e.g., List for {1,2,3}).",
        "TypeOf" => "TypeOf[expr] returns the type name of expr as a string.",
        "FreeQ" => "FreeQ[expr, pattern] returns True if pattern does not appear in expr.",
        "Cases" => {
            "Cases[{e1, e2, ...}, pattern] gives a list of elements that match pattern.\n\
             Cases[list, pattern, levelspec] — not yet supported."
        }
        "DeleteCases" => {
            "DeleteCases[{e1, e2, ...}, pattern] removes elements that match pattern.\n\
             DeleteCases[list, pattern, levelspec] — not yet supported."
        }
        "Dispatch" => {
            "Dispatch[rules] builds a dispatch-indexed rule set for O(1) lookup by head name and argument type patterns. Use for large rule sets like Rubi."
        }

        // ── String ──
        "StringJoin" => "StringJoin[s1, s2, ...] or s1 <> s2 <> ... concatenates strings.",
        "StringLength" => "StringLength[s] gives the number of characters in string s.",
        "ToString" => "ToString[expr] converts expr to a string representation.",
        "ToExpression" => "ToExpression[s] parses string s as Syma code and evaluates it.",
        "StringSplit" => "StringSplit[s] splits string s into a list of substrings.",
        "StringReplace" => "StringReplace[s, rules] applies string replacement rules.",
        "StringTake" => "StringTake[s, n] gives the first n characters of s.",
        "StringDrop" => "StringDrop[s, n] gives s with the first n characters removed.",
        "StringContainsQ" => "StringContainsQ[s, sub] returns True if s contains substring sub.",
        "StringReverse" => "StringReverse[s] reverses the characters in string s.",
        "ToUpperCase" => "ToUpperCase[s] converts string s to uppercase.",
        "ToLowerCase" => "ToLowerCase[s] converts string s to lowercase.",
        "Characters" => "Characters[s] gives a list of the characters in string s.",
        "StringMatchQ" => "StringMatchQ[s, pattern] returns True if s matches the string pattern.",
        "StringPadLeft" => "StringPadLeft[s, n] pads string s on the left to length n.",
        "StringPadRight" => "StringPadRight[s, n] pads string s on the right to length n.",
        "StringTrim" => "StringTrim[s] removes whitespace from the beginning and end of s.",
        "StringStartsQ" => "StringStartsQ[s, prefix] returns True if s starts with prefix.",
        "StringEndsQ" => "StringEndsQ[s, suffix] returns True if s ends with suffix.",

        // ── Extended string operations ──
        "StringPart" => {
            "StringPart[s, n] gives the n-th character in string s (1-indexed).\nStringPart[s, -n] counts from the end."
        }
        "StringPosition" => {
            "StringPosition[s, sub] gives a list of the starting positions where sub appears in s."
        }
        "StringCount" => {
            "StringCount[s, sub] gives the number of times sub appears as a substring of s."
        }
        "StringRepeat" => "StringRepeat[s, n] repeats string s n times.",
        "StringDelete" => "StringDelete[s, sub] deletes all occurrences of sub from s.",
        "StringInsert" => {
            "StringInsert[s, ins, n] inserts string ins into s at position n (1-indexed).\nStringInsert[s, ins, -n] counts from the end."
        }
        "StringRiffle" => {
            "StringRiffle[list, sep] joins the string representations of the elements in list, inserting sep between each."
        }
        "StringFreeQ" => {
            "StringFreeQ[s, sub] returns True if s does NOT contain the substring sub."
        }
        "LetterQ" => "LetterQ[s] returns True if all characters in s are letters.",
        "DigitQ" => "DigitQ[s] returns True if all characters in s are digits.",
        "UpperCaseQ" => "UpperCaseQ[s] returns True if all letters in s are uppercase.",
        "LowerCaseQ" => "LowerCaseQ[s] returns True if all letters in s are lowercase.",
        "TextWords" => "TextWords[s] gives the list of words in string s (split by whitespace).",
        "CharacterCounts" => {
            "CharacterCounts[s] returns a list of {character, count} pairs for each distinct character in s."
        }
        "Alphabet" => {
            "Alphabet[] gives the list of lowercase letters a–z.\nAlphabet[\"Latin\"] gives the same."
        }

        // ── Math ──
        "Sin" => "Sin[z] gives the sine of z.",
        "Cos" => "Cos[z] gives the cosine of z.",
        "Tan" => "Tan[z] gives the tangent of z.",
        "Log" => {
            "Log[z] gives the natural logarithm of z (logarithm to base e).\nLog[b, z] gives the logarithm to base b."
        }
        "Exp" => "Exp[z] gives the exponential of z (e^z).",
        "Sqrt" => "Sqrt[z] gives the square root of z.",
        "Floor" => "Floor[x] gives the greatest integer less than or equal to x.",
        "Ceiling" => "Ceiling[x] gives the least integer greater than or equal to x.",
        "Round" => "Round[x] rounds x to the nearest integer.",
        "Max" => "Max[x, y, ...] gives the numerically largest of the arguments.",
        "Min" => "Min[x, y, ...] gives the numerically smallest of the arguments.",
        "ArcSin" => "ArcSin[z] gives the inverse sine of z.",
        "ArcCos" => "ArcCos[z] gives the inverse cosine of z.",
        "ArcTan" => "ArcTan[z] gives the inverse tangent of z.",
        "Csc" => "Csc[z] gives the cosecant of z (1/Sin[z]).",
        "Sec" => "Sec[z] gives the secant of z (1/Cos[z]).",
        "Cot" => "Cot[z] gives the cotangent of z (1/Tan[z]).",
        "ArcCsc" => "ArcCsc[z] gives the inverse cosecant of z.",
        "ArcSec" => "ArcSec[z] gives the inverse secant of z.",
        "ArcCot" => "ArcCot[z] gives the inverse cotangent of z.",
        "Haversine" => "Haversine[z] gives the haversine of z, (1 - Cos[z])/2.",
        "InverseHaversine" => {
            "InverseHaversine[z] gives the inverse haversine of z, 2 ArcSin[Sqrt[z]]."
        }
        "SinDegrees" => "SinDegrees[θ] gives the sine of θ degrees.",
        "CosDegrees" => "CosDegrees[θ] gives the cosine of θ degrees.",
        "TanDegrees" => "TanDegrees[θ] gives the tangent of θ degrees.",
        "CscDegrees" => "CscDegrees[θ] gives the cosecant of θ degrees.",
        "SecDegrees" => "SecDegrees[θ] gives the secant of θ degrees.",
        "CotDegrees" => "CotDegrees[θ] gives the cotangent of θ degrees.",
        "ArcSinDegrees" => "ArcSinDegrees[z] gives the inverse sine of z in degrees.",
        "ArcCosDegrees" => "ArcCosDegrees[z] gives the inverse cosine of z in degrees.",
        "ArcTanDegrees" => "ArcTanDegrees[z] gives the inverse tangent of z in degrees.",
        "ArcCscDegrees" => "ArcCscDegrees[z] gives the inverse cosecant of z in degrees.",
        "ArcSecDegrees" => "ArcSecDegrees[z] gives the inverse secant of z in degrees.",
        "ArcCotDegrees" => "ArcCotDegrees[z] gives the inverse cotangent of z in degrees.",
        "Log2" => "Log2[z] gives the base-2 logarithm of z.",
        "Log10" => "Log10[z] gives the base-10 logarithm of z.",
        "Mod" => "Mod[m, n] gives the remainder when m is divided by n.",
        "GCD" => "GCD[n1, n2, ...] gives the greatest common divisor of the arguments.",
        "LCM" => "LCM[n1, n2, ...] gives the least common multiple of the arguments.",
        "Factorial" => {
            "Factorial[n] or n! gives the factorial of n. For non-integer n, returns Gamma[1 + n]."
        }
        "Gamma" => "Gamma[z] gives the Euler gamma function of z.",
        "OddQ" => "OddQ[n] returns True if n is an odd integer, False otherwise.",
        "EvenQ" => "EvenQ[n] returns True if n is an even integer, False otherwise.",
        "PosQ" => "PosQ[x] returns True if x > 0.",
        "NegQ" => "NegQ[x] returns True if x < 0.",
        "PositiveQ" => "PositiveQ[x] returns True if x > 0 (works for Integer, Real, Rational).",
        "NegativeQ" => "NegativeQ[x] returns True if x < 0 (works for Integer, Real, Rational).",
        "NonNegativeQ" => "NonNegativeQ[x] returns True if x >= 0 (works for Integer, Real, Rational).",
        "ZeroQ" => "ZeroQ[x] returns True if x == 0 (works for Integer, Real, Rational).",
        "InverseErf" => "InverseErf[x] gives the inverse error function of x.",
        "InverseErfc" => "InverseErfc[x] gives the inverse complementary error function of x.",
        "LogGamma" => "LogGamma[z] gives the natural logarithm of the gamma function.",
        "GammaRegularized" => "GammaRegularized[a, z] gives the regularized incomplete gamma function P(a,z).",
        "AiryBi" => "AiryBi[z] gives the Airy function Bi(z).",
        // ── Symbolic ──
        "Simplify" => "Simplify[expr] attempts to simplify expr. (Currently a pass-through.)",
        "Expand" => "Expand[expr] expands products and powers in expr. (Currently a pass-through.)",
        "D" => "D[f, x] gives the partial derivative of f with respect to x. (Planned.)",
        "Integrate" => {
            "Integrate[f, x] computes the indefinite integral of f with respect to x.\n\
                         Supports: polynomials, sin, cos, exp, tan, sec², csc², sum rule,\n\
                         constant factor extraction, and linear substitution."
        }
        "Factor" => "Factor[expr] factors the polynomial expr. (Planned.)",
        "Solve" => "Solve[eqns, vars] solves equations for variables. (Planned.)",
        "Series" => {
            "Series[expr, {x, x0, n}] computes a power series expansion to order n.\n\
             Returns a SeriesData object that displays with an O[x-x0]^(n+1) remainder term."
        }

        // ── Discrete Calculus ──
        "DiscreteDelta" => {
            "DiscreteDelta[n1, n2, ...] returns 1 if all arguments are zero, 0 otherwise."
        }
        "DiscreteShift" => {
            "DiscreteShift[expr, n] represents the forward shift of expr with respect to n.\n\
             DiscreteShift[expr, n, h] shifts by step h."
        }
        "DiscreteRatio" => {
            "DiscreteRatio[expr, n] represents the ratio of expr at successive points of n.\n\
             DiscreteRatio[expr, n, h] uses step h."
        }
        "FactorialPower" => {
            "FactorialPower[x, n] gives the falling factorial x^(n) = x*(x-1)*...*(x-n+1).\n\
             FactorialPower[x, n, h] uses step h."
        }
        "BernoulliB" => "BernoulliB[n] gives the n-th Bernoulli number B_n.",
        "LinearRecurrence" => {
            "LinearRecurrence[kernel, init, n] gives the n-th term of a linear recurrence with kernel coefficients and initial values."
        }
        "RecurrenceTable" => {
            "RecurrenceTable[eqns, f, {n, nmin, nmax}] generates a list of values from recurrence equations.\n\
             Example: RecurrenceTable[{a[1] == 1, a[n+1] == 2*a[n]}, a, {n, 1, 5}]"
        }
        "RSolve" => "RSolve[eqn, f[n], n] attempts to solve a recurrence equation for f[n].",

        // ── Combinatorics ──
        "Binomial" => "Binomial[n, k] gives the binomial coefficient C(n, k).",
        "Multinomial" => "Multinomial[n1, n2, ...] gives the multinomial coefficient (n1+n2+...)! / (n1! n2! ...).",
        "Factorial2" => "Factorial2[n] or n!! gives the double factorial n*(n-2)*(n-4)*....",
        "AlternatingFactorial" => "AlternatingFactorial[n] gives the alternating factorial sum((-1)^(k+1) * k!, k=1..n).",
        "Subfactorial" => "Subfactorial[n] gives the number of derangements of n elements.",
        "Permutations" => "Permutations[list] gives all possible reorderings of list.\nPermutations[list, n] gives permutations of length n.",
        "Subsets" => "Subsets[list] gives all subsets of list.\nSubsets[list, n] gives subsets of up to size n.\nSubsets[list, {n}] gives subsets of exactly size n.",
        "Tuples" => "Tuples[{e1, e2, ...}, n] generates all n-tuples of elements from the given list.",
        "Arrangements" => "Arrangements[list, n] gives all permutations of length n from list (ordered subsets).",
        "StirlingS1" => "StirlingS1[n, k] gives the (signed) Stirling number of the first kind.",
        "StirlingS2" => "StirlingS2[n, k] gives the Stirling number of the second kind.",
        "LucasL" => "LucasL[n] gives the n-th Lucas number.\nLucasL[n, x] gives the Lucas polynomial L_n(x).",
        "Fibonacci" => "Fibonacci[n] gives the n-th Fibonacci number.\nFibonacci[n, x] gives the Fibonacci polynomial F_n(x).",
        "CatalanNumber" => "CatalanNumber[n] gives the n-th Catalan number C_n = Binomial(2n, n) / (n+1).",
        "HarmonicNumber" => "HarmonicNumber[n] gives the n-th harmonic number H_n = sum(1/k, k=1..n).\nHarmonicNumber[n, r] gives sum(1/k^r, k=1..n).",
        "PartitionsP" => "PartitionsP[n] gives the number of unrestricted integer partitions of n.",
        "PartitionsQ" => "PartitionsQ[n] gives the number of partitions of n into distinct parts.",
        "BellB" => "BellB[n] gives the n-th Bell number.\nBellB[n, k] gives the partial Bell polynomial B_{n,k}.",

        // ── Calendar / Date & Time ──
        "DateObject" => "DateObject[{y, m, d}] creates a date object.\nDateObject[{y, m, d, h, mi, s}] includes time.",
        "DateString" => "DateString[] gives the current date/time as a string.\nDateString[format] uses the specified format.",
        "DateList" => "DateList[] gives the current date/time as {y, m, d, h, mi, s}.\nDateList[timestamp] converts a Unix timestamp.",
        "DatePlus" => "DatePlus[date, n] adds n days to date.\nDatePlus[date, {n, \"Month\"}] adds n months.",
        "DateDifference" => "DateDifference[d1, d2] gives the number of days between two dates.",
        "Now" => "Now gives the current date/time as a DateObject.",
        "Today" => "Today gives the current date as a DateObject.",
        "DayName" => "DayName[] gives the day of the week for today.\nDayName[date] gives the day for the given date.",
        "AbsoluteTime" => "AbsoluteTime[] gives the current Unix timestamp in seconds.\nAbsoluteTime[date] converts a date to Unix timestamp.",
        "LeapYearQ" => "LeapYearQ[year] returns True if the given year is a leap year.",
        "DayCount" => "DayCount[d1, d2] gives the number of days from d1 to d2.",
        "MonthName" => "MonthName[] gives the name of the current month.\nMonthName[n] gives the name of the n-th month.",

        // ── Control ──
        "FixedPoint" => {
            "FixedPoint[f, expr] applies f repeatedly until the result no longer changes.\nFixedPoint[f, expr, n] performs at most n iterations."
        }

        // ── Package loading ──
        "Needs" => {
            "Needs[\"PackageName\"] loads a standard library package and makes its symbols available.\n\
             Built-in packages: LinearAlgebra, Statistics, Graphics.\n\
             Returns Null if the package is already loaded."
        }

        // ── LinearAlgebra ──
        "Dimensions" => {
            "Dimensions[m] gives the dimensions of a matrix or vector as a list {rows, cols}."
        }
        "Dot" => "Dot[a, b] or a . b computes the dot product of vectors or matrix multiplication.",
        "MatrixMultiply" => "MatrixMultiply[a, b] is an alias for Dot[a, b].",
        "IdentityMatrix" => "IdentityMatrix[n] gives the n×n identity matrix.",
        "Det" => "Det[m] computes the determinant of a square matrix.",
        "Inverse" => "Inverse[m] computes the inverse of a square matrix.",
        "Tr" => "Tr[m] gives the trace (sum of diagonal elements) of a matrix.",
        "Norm" => "Norm[v] gives the Euclidean norm of a vector or Frobenius norm of a matrix.",
        "Cross" => "Cross[a, b] computes the cross product of two 3D vectors.",
        "LinearSolve" => "LinearSolve[A, b] solves the linear system A·x = b for x.",
        "Eigenvalues" => "Eigenvalues[m] gives the eigenvalues of matrix m via QR iteration.",
        "MatrixPower" => "MatrixPower[m, n] gives the n-th matrix power of m.",
        "ArrayFlatten" => {
            "ArrayFlatten[{{m11, m12}, {m21, m22}}] flattens a matrix of matrices into a single matrix."
        }

        // ── Statistics ──
        "Mean" => "Mean[list] gives the arithmetic mean of the elements in list.",
        "Median" => "Median[list] gives the median of the elements in list.",
        "Variance" => {
            "Variance[list] gives the sample variance (with Bessel's correction, n-1 denominator)."
        }
        "StandardDeviation" => "StandardDeviation[list] gives the sample standard deviation.",
        "Quantile" => "Quantile[list, q] gives the q-th quantile of list (0 ≤ q ≤ 1).",
        "Covariance" => "Covariance[list1, list2] gives the sample covariance of two lists.",
        "Correlation" => {
            "Correlation[list1, list2] gives the Pearson correlation coefficient of two lists."
        }
        "RandomVariate" => {
            "RandomVariate[dist, n] generates n random values from distribution dist."
        }
        "NormalDistribution" => {
            "NormalDistribution[μ, σ] represents a normal distribution with mean μ and standard deviation σ."
        }
        "UniformDistribution" => {
            "UniformDistribution[min, max] represents a uniform distribution on [min, max]."
        }
        "PoissonDistribution" => {
            "PoissonDistribution[λ] represents a Poisson distribution with rate λ."
        }
        "GeometricMean" => "GeometricMean[list] gives the geometric mean of the elements in list.",
        "HarmonicMean" => "HarmonicMean[list] gives the harmonic mean of the elements in list.",
        "Skewness" => "Skewness[list] gives the skewness of the elements in list.",
        "Kurtosis" => "Kurtosis[list] gives the excess kurtosis of the elements in list.",
        "BinCounts" => "BinCounts[list, width] counts elements in bins of the given width.",
        "HistogramList" => {
            "HistogramList[list, n] gives {binEdges, counts} for n equal-width bins."
        }

        // ── Graphics ──
        "Plot" => {
            "Plot[f, {x, xmin, xmax}] plots f as a function of x from xmin to xmax.\n\
             Options: ImageSize → {width, height}, Axes → True, PlotRange → {ymin, ymax}."
        }
        "ListPlot" => {
            "ListPlot[data] plots a list of points.\n\
             ListPlot[{{x1,y1}, {x2,y2}, ...}] plots (x,y) pairs.\n\
             ListPlot[{y1, y2, ...}] plots points at x = 1, 2, ...."
        }
        "ListLinePlot" => {
            "ListLinePlot[data] plots data as connected line segments.\n\
             Accepts the same formats as ListPlot."
        }
        "ExportGraphics" => "ExportGraphics[path, svg] writes an SVG string to the file at path.",
        "Graphics" => "Graphics[primitives, options] wraps graphical primitives for rendering.",
        "Show" => "Show[graphics, options] displays graphics with updated options.",
        "Line" => {
            "Line[{{x1,y1}, {x2,y2}, ...}] represents a line primitive connecting the given points."
        }
        "Point" => "Point[{x, y}] represents a point primitive at the given coordinates.",
        "Circle" => "Circle[{cx, cy}, r] represents a circle primitive with center and radius.",
        "Rectangle" => "Rectangle[{xmin, ymin}, {xmax, ymax}] represents a rectangle primitive.",
        "RGBColor" => "RGBColor[r, g, b] specifies a color with red, green, blue components (0–1).",
        "Hue" => {
            "Hue[h, s, b] specifies a color via HSV (hue, saturation, brightness) in 0–1.\n\
             Hue[h] uses saturation=1, brightness=1. Hue[h, s] uses brightness=1."
        }
        "Thickness" => "Thickness[t] specifies line thickness.",
        "PointSize" => "PointSize[r] specifies point radius.",
        "Opacity" => "Opacity[a] specifies opacity (0 = transparent, 1 = opaque).",
        "Directive" => "Directive[style1, style2, ...] combines multiple graphics directives.",
        "GrayLevel" => "GrayLevel[g] specifies a grayscale color with intensity g (0–1).",
        "Lighter" => {
            "Lighter[color] lightens a color by 1/3 toward white.\n\
             Lighter[color, amount] lightens by the given amount (0–1)."
        }
        "Darker" => {
            "Darker[color] darkens a color by 1/3 toward black.\n\
             Darker[color, amount] darkens by the given amount (0–1)."
        }
        "Blend" => {
            "Blend[{c1, c2, ...}] averages a list of colors equally.\n\
             Blend[{c1, c2, ...}, {w1, w2, ...}] computes a weighted average."
        }
        "ColorNegate" => "ColorNegate[color] inverts an RGB color (1-r, 1-g, 1-b).",
        "Disk" => {
            "Disk[{cx, cy}, r] represents a filled circle with center and radius.\n\
             Disk[{cx, cy}, {rx, ry}] represents a filled ellipse."
        }
        "Triangle" => "Triangle[{p1, p2, p3}] represents a triangle from three vertices.",
        "Polygon" => "Polygon[{p1, p2, ...}] represents a polygon from a list of vertices.",
        "Sphere" => {
            "Sphere[{x, y, z}, r] represents a 3D sphere with center and radius.\n\
             Sphere[{x, y, z}] uses radius 1."
        }
        "Cylinder" => {
            "Cylinder[{{x1,y1,z1}, {x2,y2,z2}}, r] represents a cylinder between two 3D points.\n\
             Cylinder[{{x1,y1,z1}, {x2,y2,z2}}] uses radius 1."
        }

        // ── I/O ──
        "Print" => "Print[expr] prints expr followed by a newline to standard output.",
        "Input" => "Input[] reads a line of input from the user.",
        "Write" => "Write[stream, expr] writes expr to an output stream.",
        "WriteLine" => "WriteLine[stream, s] writes string s followed by a newline.",
        "PrintF" => "PrintF[fmt, args...] prints formatted output.",
        "WriteString" => {
            "WriteString[path, data] writes the string data to the file at path, creating or overwriting it."
        }
        "ReadString" => {
            "ReadString[path] reads the entire file at path and returns it as a string."
        }
        "Export" => {
            "Export[path, data] exports data to a file.\n\
             Format is detected by extension, or provide an explicit 3rd argument:\n\
             Export[path, data, \"format\"].\n\
             Supported formats: JSON, CSV, TSV, Table, SVG, PNG, Text, WL."
        }
        "Import" => {
            "Import[path] imports data from a file, detecting format from extension.\n\
             Import[path, \"format\"] specifies the format explicitly.\n\
             Supported formats: JSON, CSV, TSV, Table, HTML, PNG, SVG, WL, NB, Text."
        }
        "ImportString" => {
            "ImportString[data, \"format\"] imports string data using the specified format.\n\
             Supported formats: JSON, CSV, TSV, Table, HTML, SVG, Text."
        }
        "ExportString" => {
            "ExportString[data, \"format\"] exports data to a string using the specified format.\n\
             Supported formats: JSON, CSV, TSV, Table, SVG, Text."
        }
        "ReadList" => {
            "ReadList[path] reads all lines from a file and returns them as a list of strings."
        }

        // ── Error handling ──
        "Throw" => "Throw[expr] throws expr as an exception, to be caught by Catch.",
        "Error" => "Error[msg] raises an error with the given message.",

        // ── Association ──
        "Keys" => "Keys[assoc] gives a list of the keys in an association.",
        "Values" => "Values[assoc] gives a list of the values in an association.",
        "Lookup" => {
            "Lookup[assoc, key] gives the value associated with key, or Missing if not found."
        }
        "KeyExistsQ" => "KeyExistsQ[assoc, key] returns True if key exists in the association.",
        "AssociationQ" => "AssociationQ[expr] returns True if expr is a valid association.",
        "Normal" => "Normal[assoc] converts an association to a list of rules.",
        "KeySort" => "KeySort[assoc] sorts the keys of an association alphabetically.",
        "KeySortBy" => "KeySortBy[assoc, f] sorts keys using the ordering function f.",
        "KeyTake" => "KeyTake[assoc, keys] returns an association with only the specified keys.",
        "KeyDrop" => "KeyDrop[assoc, keys] returns an association without the specified keys.",
        "KeySelect" => "KeySelect[assoc, pred] selects entries where pred[key] returns True.",
        "KeyMap" => "KeyMap[f, assoc] applies f to each key in the association.",
        "KeyValueMap" => {
            "KeyValueMap[f, assoc] applies f to each {key, value} pair, returning a list."
        }
        "KeyMemberQ" => "KeyMemberQ[assoc, key] returns True if the key exists in the association.",
        "KeyFreeQ" => {
            "KeyFreeQ[assoc, key] returns True if the key does NOT exist in the association."
        }
        "AssociateTo" => {
            "AssociateTo[assoc, rule] returns a new association with the key->value added.\n\
             AssociateTo[assoc, {rule1, rule2, ...}] adds multiple entries."
        }
        "KeyDropFrom" => {
            "KeyDropFrom[assoc, key] returns a new association with the specified key removed."
        }
        "Counts" => "Counts[list] returns an association counting occurrences of each element.",
        "CountsBy" => "CountsBy[list, f] counts occurrences grouped by f[element].",
        "GroupBy" => "GroupBy[list, f] groups elements of list by f[element].",
        "Merge" => {
            "Merge[{assoc1, assoc2, ...}, combiner] merges associations, using combiner for duplicate keys."
        }
        "KeyUnion" => {
            "KeyUnion[{assoc1, assoc2, ...}] returns the union of keys from multiple associations."
        }
        "KeyIntersection" => {
            "KeyIntersection[{assoc1, assoc2, ...}] returns the intersection of keys."
        }
        "KeyComplement" => {
            "KeyComplement[assoc1, assoc2] returns keys in assoc1 that are not in assoc2."
        }

        // ── Dataset ──
        "Dataset" => {
            "Dataset[data] creates a Dataset wrapper around structured data for pretty display and query operations.\nUse call syntax: ds[All, \"col\"], ds[i], ds[i, \"col\"]."
        }
        "DatasetQ" => "DatasetQ[x] returns True if x is a Dataset, False otherwise.",
        "SortBy" => {
            "SortBy[list, f] sorts list elements by the key produced by applying f to each element."
        }
        "JoinAcross" => {
            "JoinAcross[list1, list2, key] performs an inner join of two lists of associations on the specified key."
        }

        // ── Random ──
        "RandomInteger" => {
            "RandomInteger[n] gives a random integer between 0 and n.\nRandomInteger[{min, max}] gives a random integer between min and max."
        }
        "RandomReal" => {
            "RandomReal[] gives a random real number between 0 and 1.\nRandomReal[{min, max}] gives a random real between min and max."
        }
        "RandomChoice" => "RandomChoice[list] gives a pseudorandom element from list.",

        // ── Constants ──
        "Pi" => {
            "Pi is the constant \u{03c0} (3.14159...), the ratio of a circle's circumference to its diameter."
        }
        "E" => "E is Euler's number e (2.71828...), the base of the natural logarithm.",
        "I" => "I is the imaginary unit, satisfying I^2 = -1.",
        "Degree" => {
            "Degree is the constant Pi/180, used to convert degrees to radians. E.g., 30 Degree = Pi/6."
        }
        "Null" => "Null represents the absence of an expression or result.",
        "True" => "True represents the logical value true.",
        "False" => "False represents the logical value false.",

        // ── Sequence ──
        "Sequence" => {
            "Sequence[expr1, expr2, ...] represents a sequence of arguments \
             that automatically splices into function calls.\n\
             Sequence[] evaporates entirely; Sequence[expr] acts like Identity.\n\
             Most functions automatically splice Sequence; those with \
             SequenceHold or HoldAllComplete do not."
        }

        // ── Control flow ──
        "If" => {
            "If[cond, t, f] evaluates t if cond is True, f if False.\nIf[cond, t] evaluates t if cond is True, returns Null otherwise."
        }
        "Which" => {
            "Which[test1, val1, test2, val2, ...] evaluates each test in order and returns the val corresponding to the first True test."
        }
        "Switch" => {
            "Switch[expr, form1, val1, form2, val2, ...] evaluates expr and returns the val matching the first matching form."
        }
        "For" => "For[init, test, step, body] executes a for loop.",
        "While" => "While[test, body] evaluates body while test is True.",
        "Do" => {
            "Do[expr, {i, max}] evaluates expr max times.\nDo[expr, {i, min, max}] evaluates expr for i from min to max."
        }
        "Function" => "Function[{params}, body] creates a pure function with named parameters.",
        "Hold" => "Hold[expr] prevents evaluation of expr.",
        "HoldComplete" => {
            "HoldComplete[expr] prevents evaluation and attribute processing of expr."
        }
        "Catch" => "Catch[expr] evaluates expr, returning any value passed to Throw.",
        "Return" => {
            "Return[expr] returns expr from the enclosing function.\nReturn[] returns Null."
        }
        "Break" => "Break[] exits the enclosing For, While, or Do loop.",
        "Continue" => "Continue[] skips to the next iteration of the enclosing loop.",
        "N" => {
            "N[expr] evaluates expr numerically.\nN[expr, prec] uses prec decimal digits of precision."
        }

        // ── Parallel ──
        "ParallelMap" => {
            "ParallelMap[f, list] applies f to each element of list in parallel, returning a list of results."
        }
        "ParallelTable" => {
            "ParallelTable[expr, {i, min, max}] evaluates expr for i from min to max in parallel.\n\
             ParallelTable[expr, {i, max}] evaluates expr for i from 1 to max in parallel."
        }
        "KernelCount" => "KernelCount returns the number of available parallel worker threads.",
        "LaunchKernels" => {
            "LaunchKernels[] returns the current kernel count.\nLaunchKernels[n] sets the number of parallel workers to n."
        }
        "CloseKernels" => "CloseKernels[] resets the parallel worker pool. Returns Null.",
        "ParallelSum" => {
            "ParallelSum[expr, {i, min, max}] evaluates a parallel sum of expr as i goes from min to max.\n\
             ParallelSum[expr, {i, max}] evaluates a parallel sum of expr for i from 1 to max."
        }
        "ParallelEvaluate" => {
            "ParallelEvaluate[expr] evaluates expr on each parallel worker, returning a list of results."
        }
        "ParallelTry" => {
            "ParallelTry[list] evaluates each element of list in parallel, returning the first result obtained.\n\
             ParallelTry[f, list] applies f to each element of list in parallel, returning the first result."
        }
        "ParallelProduct" => {
            "ParallelProduct[expr, {i, min, max}] evaluates a parallel product of expr as i goes from min to max.\n\
             ParallelProduct[expr, {i, max}] evaluates a parallel product of expr for i from 1 to max."
        }
        "ParallelDo" => {
            "ParallelDo[expr, {i, min, max}] evaluates expr for i from min to max in parallel, returning Null.\n\
             ParallelDo[expr, {i, max}] evaluates expr for i from 1 to max in parallel, returning Null."
        }
        "ParallelCombine" => {
            "ParallelCombine[f, list] applies binary function f to combine elements of list in parallel, returning a single result."
        }
        "ProcessorCount" => {
            "ProcessorCount returns the number of processor cores on the current computer."
        }
        "AbortKernels" => {
            "AbortKernels[] aborts all running kernel evaluations. (Currently a no-op.)"
        }

        // ── Format/display ──
        "InputForm" => {
            "InputForm[expr] displays expr using infix notation (e.g., `a + b` instead of `Plus[a, b]`)."
        }
        "FullForm" => "FullForm[expr] displays expr in head[arg, ...] notation.",
        "StandardForm" => {
            "StandardForm[expr] displays expr in StandardForm (infix notation with SeriesData special display)."
        }
        "OutputForm" => {
            "OutputForm[expr] displays expr in OutputForm (plain-text, same as StandardForm for terminal)."
        }
        "Short" => {
            "Short[expr] displays expr with top-level truncation (shows at most 5 items).\nShort[expr, n] displays at most n top-level items."
        }
        "Shallow" => {
            "Shallow[expr] displays expr with limited nesting depth (default 3).\nShallow[expr, n] limits nesting to n levels."
        }
        "NumberForm" => "NumberForm[expr, n] displays numbers with n significant digits.",
        "ScientificForm" => {
            "ScientificForm[expr, n] displays numbers in scientific notation with n significant digits."
        }
        "BaseForm" => "BaseForm[expr, base] displays a number in the given base (2–36).",
        "Grid" => "Grid[list] displays a 2D list as an aligned table grid.",
        "Defer" => "Defer[expr] displays expr in its original form. (Currently a display wrapper.)",
        "SyntaxQ" => {
            "SyntaxQ[\"expr\"] returns True if expr is valid Syma syntax, False otherwise. Performs lex + parse only (no evaluation)."
        }
        "SyntaxLength" => {
            "SyntaxLength[\"expr\"] returns the position of the first syntax error, or the length of the string if valid."
        }
        "TableForm" => {
            "TableForm[list] formats a list as a table. For 1D lists, each element is on its own line.\n\
             For 2D lists, elements are formatted as aligned columns separated by tabs."
        }
        "MatrixForm" => {
            "MatrixForm[list] formats a 2D list as a matrix with bracket notation.\n\
             Each row is on its own line, elements are aligned in columns."
        }
        "PaddedForm" => {
            "PaddedForm[expr, n] formats expr padded to n digits total.\n\
             PaddedForm[expr, {n, f}] uses n digits total with f digits after the decimal point."
        }
        "StringForm" => {
            "StringForm[\"template\", arg1, arg2, ...] replaces `1`, `2`, etc. in the template\n\
             with the corresponding arguments. Similar to printf-style formatting.\n\
             Example: StringForm[\"Hello `1`, age `2`\", \"Alice\", 30] → \"Hello Alice, age 30\"."
        }

        // ── Persistent storage ──
        "LocalSymbol" => {
            "LocalSymbol[\"name\"] reads a persisted value from ~/.syma/LocalSymbols/.\n\
             LocalSymbol[\"name\", default] returns default if the key does not exist.\n\
             LocalSymbol[\"name\"] = value assigns a value to a persistent key.\n\
             Supported value types: Integer, Real, String, Bool, Null, List, Assoc."
        }

        // ── File system ──
        "FileNameSplit" => {
            "FileNameSplit[\"path\"] splits a file name into a list of its components."
        }
        "FileNameJoin" => {
            "FileNameJoin[{\"comp1\", \"comp2\", ...}] joins path components into a file name."
        }
        "FileNameTake" => "FileNameTake[\"path\", n] gives the last n components of the path.",
        "FileNameDrop" => {
            "FileNameDrop[\"path\", n] gives the path with the last n components removed."
        }
        "FileBaseName" => "FileBaseName[\"path\"] gives the file name without its extension.",
        "FileExtension" => "FileExtension[\"path\"] gives the file extension (e.g., \"txt\").",
        "FileNameDepth" => "FileNameDepth[\"path\"] gives the number of path components.",
        "DirectoryName" => "DirectoryName[\"path\"] gives the directory portion of the path.",
        "ParentDirectory" => "ParentDirectory[\"path\"] gives the parent directory of the path.",
        "ExpandFileName" => "ExpandFileName[\"path\"] resolves the path to an absolute file name.",
        "FileExistsQ" => "FileExistsQ[\"path\"] returns True if the file exists.",
        "DirectoryQ" => "DirectoryQ[\"path\"] returns True if the path is an existing directory.",
        "FileNames" => {
            "FileNames[] lists files in the current directory.\nFileNames[\"pattern\"] lists files matching the glob pattern.\nFileNames[\"pattern\", {\"dir1\", \"dir2\"}] searches in the given directories."
        }
        "Names" => {
            "Names[] returns a sorted list of all known symbol names.\nNames[\"pattern\"] returns symbol names matching a string pattern, where * matches any sequence of characters and ? matches any single character."
        }

        // ── Symbol Clearing ──
        "Clear" => {
            "Clear[sym1, sym2, ...] removes definitions, values, and attributes for each symbol. Protected symbols are not affected."
        }
        "ClearAll" => {
            "ClearAll[sym1, sym2, ...] removes definitions, values, attributes, and lazy providers for each symbol. Protected symbols are not affected."
        }
        "Unset" => {
            "Unset[sym] removes the value or definition for a symbol without clearing its attributes. Protected symbols are not affected."
        }
        "Remove" => {
            "Remove[sym1, sym2, ...] completely removes symbols from the system, including bindings, attributes, and lazy providers. Removes even Protected symbols."
        }

        // ── Image Processing ──
        "Image" => {
            "Image[data] creates an image from a 2D (grayscale) or 3D (RGB/RGBA) list of values in [0,1].\n\
             Image[data, \"type\"] specifies the storage type (e.g., \"Byte\")."
        }
        "ImageData" => {
            "ImageData[image] extracts pixel data as a list of lists with values in [0,1]."
        }
        "ImageDimensions" => "ImageDimensions[image] returns {width, height} of the image.",
        "ImageType" => {
            "ImageType[image] returns the image type as a string: \"Byte\", \"Bit16\", or \"Real32\"."
        }
        "ImageResize" => {
            "ImageResize[image, {w, h}] resizes image to the given dimensions using Lanczos3 filter.\n\
             ImageResize[image, n] scales width to n pixels preserving aspect ratio."
        }
        "ImageRotate" => {
            "ImageRotate[image, angle] rotates the image by the given angle in degrees (90, 180, 270 supported natively)."
        }
        "ImageAdjust" => {
            "ImageAdjust[image] auto-stretches contrast to the full range.\n\
             ImageAdjust[image, {c, b, g}] adjusts contrast (c), brightness (b), and gamma (g)."
        }
        "Binarize" => {
            "Binarize[image] converts to black and white (threshold at 0.5).\n\
             Binarize[image, t] uses threshold t in [0,1]."
        }
        "ColorConvert" => {
            "ColorConvert[image, \"Grayscale\"] converts an image to grayscale.\n\
             ColorConvert[image, \"RGB\"] converts to RGB."
        }
        "GaussianFilter" => "GaussianFilter[image, r] applies Gaussian blur with sigma = r.",
        "EdgeDetect" => {
            "EdgeDetect[image] applies Sobel edge detection, returning an edge magnitude image."
        }
        "ImageConvolve" => {
            "ImageConvolve[image, kernel] convolves image with a 2D kernel (list of lists, odd dimensions)."
        }

        // ── Number theory help ──
        "ModularInverse" => {
            "ModularInverse[a, m] gives the modular inverse of a modulo m, or returns unevaluated if no inverse exists."
        }
        "PrimeOmega" => {
            "PrimeOmega[n] gives the total number of prime factors of n, counting multiplicities (Ω(n)).\n\
             PrimeOmega[12] = 3 (2^2 * 3^1)."
        }
        "PrimeNu" => {
            "PrimeNu[n] gives the number of distinct prime factors of n (ω(n)).\n\
             PrimeNu[12] = 2 (2 and 3)."
        }
        "DigitCount" => {
            "DigitCount[n] returns a list of digit counts for n in base 10.\n\
             DigitCount[n, base] uses the given base.\n\
             DigitCount[n, base, d] returns the count of digit d."
        }
        "JacobiSymbol" => {
            "JacobiSymbol[a, n] computes the Jacobi symbol (a/n), where n is a positive odd integer.\n\
             Returns -1, 0, or 1."
        }
        "ChineseRemainder" => {
            "ChineseRemainder[{a1, a2, ...}, {n1, n2, ...}] solves the system of congruences\n\
             x ≡ a_i (mod n_i) for pairwise coprime moduli."
        }
        "MultiplicativeOrder" => {
            "MultiplicativeOrder[a, n] gives the smallest positive integer k such that a^k ≡ 1 (mod n).\n\
             Requires gcd(a, n) = 1."
        }
        "PrimitiveRoot" => {
            "PrimitiveRoot[n] gives the smallest primitive root of n, or raises an error\n\
             if no primitive root exists."
        }
        "PerfectNumberQ" => {
            "PerfectNumberQ[n] returns True if n is a perfect number (sum of proper divisors equals n),\n\
             False otherwise."
        }
        "MangoldtLambda" => {
            "MangoldtLambda[n] returns ln(p) if n = p^k for prime p and k ≥ 1, or 0 otherwise.\n\
             The von Mangoldt function Λ(n)."
        }
        "LiouvilleLambda" => {
            "LiouvilleLambda[n] returns (-1)^Ω(n), where Ω(n) is the total number of prime factors\n\
             with multiplicity. The Liouville function λ(n)."
        }
        "DivisorSum" => {
            "DivisorSum[n, form] sums form[d] for all positive divisors d of n.\n\
             The form function must return an integer for each divisor."
        }
        "PrimePowerQ" => "PrimePowerQ[n] returns True if n is a prime power (n = p^k for prime p, k >= 1).",
        "SquareFreeQ" => "SquareFreeQ[n] returns True if n is squarefree (no prime squared divides n).",
        "CompositeQ" => "CompositeQ[n] returns True if n is composite (greater than 1 and not prime).",
        "PerfectPowerQ" => "PerfectPowerQ[n] returns True if n = a^b for some integer a >= 1 and b >= 2.",
        "IntegerExponent" => {
            "IntegerExponent[n] gives the highest power of 10 dividing n.\n\
             IntegerExponent[n, b] gives the highest power of b dividing n."
        }
        "FromDigits" => {
            "FromDigits[list] converts a list of digits to an integer in base 10.\n\
             FromDigits[list, b] converts digits in base b.\n\
             FromDigits[string] parses a string as a number."
        }
        "ToDigits" => {
            "ToDigits[n] converts integer n to a list of decimal digits.\n\
             ToDigits[n, b] converts to digits in base b."
        }
        "ContinuedFraction" => {
            "ContinuedFraction[x] gives the continued fraction representation of x.\n\
             ContinuedFraction[x, n] gives at most n terms for real numbers."
        }
        "FromContinuedFraction" => "FromContinuedFraction[list] reconstructs a number from its continued fraction representation.",
        "NumberExpand" => {
            "NumberExpand[n] gives the digit expansion of n as {d_k*10^k, ..., d_1*10, d_0}.\n\
             NumberExpand[n, b] uses base b."
        }

        // -- Developer context --
        "$MaxMachineInteger" => {
            "$MaxMachineInteger is the maximum machine-sized integer (2^63 - 1 on 64-bit systems)."
        }
        "MachineIntegerQ" => {
            "MachineIntegerQ[expr] returns True if expr is an integer that fits in a machine-sized integer."
        }
        "ToPackedArray" => {
            "ToPackedArray[list] converts a list of integers or reals to a packed array."
        }
        "FromPackedArray" => {
            "FromPackedArray[packed] converts a packed array back to a regular list."
        }
        "PackedArrayQ" => "PackedArrayQ[expr] returns True if expr is a packed array.",
        "PackedArrayForm" => "PackedArrayForm is an option symbol for PackedArray display.",
        "BesselSimplify" => {
            "BesselSimplify[expr] attempts to simplify Bessel function expressions."
        }
        "GammaSimplify" => "GammaSimplify[expr] attempts to simplify Gamma function expressions.",
        "PolyGammaSimplify" => {
            "PolyGammaSimplify[expr] attempts to simplify PolyGamma expressions."
        }
        "ZetaSimplify" => "ZetaSimplify[expr] attempts to simplify Zeta function expressions.",
        "PolyLogSimplify" => {
            "PolyLogSimplify[expr] attempts to simplify PolyLog function expressions."
        }
        "TrigToRadicals" => {
            "TrigToRadicals[expr] converts trigonometric expressions to radical form."
        }
        "CellInformation" => {
            "CellInformation[expr] returns cell information (notebook frontend not yet available)."
        }
        "NotebookConvert" => {
            "NotebookConvert[source] converts notebooks (notebook frontend not yet available)."
        }
        "ReplaceAllUnheld" => {
            "ReplaceAllUnheld[expr, rules] applies replacement rules without holding (wraps ReplaceAll)."
        }

        // -- System information --
        "$System" => {
            "$System gives the operating system and processor type for the current machine (e.g., \"MacOS-x86-64\")."
        }
        "$Version" => "$Version gives the version information for the current Syma installation.",
        "$ReleaseDate" => "$ReleaseDate gives the release date of the Syma version as a string.",
        "$Machine" => {
            "$Machine gives the processor type of the current machine (e.g., \"x86-64\")."
        }
        "$MachineName" => "$MachineName gives the network name of the current machine.",
        "$OperatingSystem" => {
            "$OperatingSystem gives the name of the operating system (e.g., \"MacOS\", \"Linux\", \"Windows\")."
        }
        "$ProcessorType" => {
            "$ProcessorType gives the processor type (e.g., \"x86-64\", \"aarch64\")."
        }
        "$User" => "$User gives the login name of the current user.",
        "$TimeZone" => "$TimeZone gives the local timezone offset from UTC in hours.",
        "$SystemId" => {
            "$SystemId gives the system identifier (e.g., \"MacOS\", \"Linux\", \"Windows\")."
        }
        "$Language" => "$Language gives the interface language (default \"English\").",
        "$CommandLine" => {
            "$CommandLine gives True if the session was started from the command line."
        }
        "$InputLine" => "$InputLine gives the text of the current input line, or Null.",

        _ => return None,
    })
}
