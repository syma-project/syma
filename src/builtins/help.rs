// ── Help documentation ──

/// Look up help text for a built-in symbol.
///
/// Returns the usage string (including attributes) if documented.
pub fn get_help(name: &str) -> Option<&'static str> {
    let entries: &[(&str, &str)] = &[
        (
            "$CommandLine",
            "$CommandLine gives True if the session was started from the command line.",
        ),
        (
            "$InputLine",
            "$InputLine gives the text of the current input line, or Null.",
        ),
        (
            "$Language",
            "$Language gives the interface language (default \"English\").",
        ),
        (
            "$Machine",
            "$Machine gives the processor type of the current machine (e.g., \"x86-64\").",
        ),
        (
            "$MachineName",
            "$MachineName gives the network name of the current machine.",
        ),
        (
            "$MaxMachineInteger",
            "$MaxMachineInteger is the maximum machine-sized integer (2^63 - 1 on 64-bit systems).",
        ),
        (
            "$OperatingSystem",
            "$OperatingSystem gives the name of the operating system (e.g., \"MacOS\", \"Linux\", \"Windows\").",
        ),
        (
            "$ProcessorType",
            "$ProcessorType gives the processor type (e.g., \"x86-64\", \"aarch64\").",
        ),
        (
            "$ReleaseDate",
            "$ReleaseDate gives the release date of the Syma version as a string.",
        ),
        (
            "$System",
            "$System gives the operating system and processor type for the current machine (e.g., \"MacOS-x86-64\").",
        ),
        (
            "$SystemId",
            "$SystemId gives the system identifier (e.g., \"MacOS\", \"Linux\", \"Windows\").",
        ),
        (
            "$TimeZone",
            "$TimeZone gives the local timezone offset from UTC in hours.",
        ),
        ("$User", "$User gives the login name of the current user."),
        (
            "$Version",
            "$Version gives the version information for the current Syma installation.",
        ),
        (
            "AbortKernels",
            "AbortKernels[] aborts all running kernel evaluations. (Currently a no-op.)",
        ),
        ("Abs", "Abs[x] gives the absolute value of x."),
        (
            "AbsoluteTime",
            "AbsoluteTime[] gives the current Unix timestamp in seconds.\nAbsoluteTime[date] converts a date to Unix timestamp.",
        ),
        (
            "Accumulate",
            "Accumulate[list] computes the running total (cumulative sum) of list elements.",
        ),
        ("AiryBi", "AiryBi[z] gives the Airy function Bi(z)."),
        (
            "Alphabet",
            "Alphabet[] gives the list of lowercase letters a–z.\nAlphabet[\"Latin\"] gives the same.",
        ),
        (
            "AlternatingFactorial",
            "AlternatingFactorial[n] gives the alternating factorial sum((-1)^(k+1) * k!, k=1..n).",
        ),
        (
            "And",
            "And[a, b, ...] or a && b && ... evaluates arguments left to right, returning \
             the first value that is False, or the last value if all are True.\n\
             And[] = True.",
        ),
        (
            "Append",
            "Append[expr, elem] returns expr with elem appended.",
        ),
        ("ArcCos", "ArcCos[z] gives the inverse cosine of z."),
        (
            "ArcCosDegrees",
            "ArcCosDegrees[z] gives the inverse cosine of z in degrees.",
        ),
        ("ArcCot", "ArcCot[z] gives the inverse cotangent of z."),
        (
            "ArcCotDegrees",
            "ArcCotDegrees[z] gives the inverse cotangent of z in degrees.",
        ),
        ("ArcCsc", "ArcCsc[z] gives the inverse cosecant of z."),
        (
            "ArcCscDegrees",
            "ArcCscDegrees[z] gives the inverse cosecant of z in degrees.",
        ),
        ("ArcSec", "ArcSec[z] gives the inverse secant of z."),
        (
            "ArcSecDegrees",
            "ArcSecDegrees[z] gives the inverse secant of z in degrees.",
        ),
        ("ArcSin", "ArcSin[z] gives the inverse sine of z."),
        (
            "ArcSinDegrees",
            "ArcSinDegrees[z] gives the inverse sine of z in degrees.",
        ),
        ("ArcTan", "ArcTan[z] gives the inverse tangent of z."),
        (
            "ArcTanDegrees",
            "ArcTanDegrees[z] gives the inverse tangent of z in degrees.",
        ),
        (
            "Arrangements",
            "Arrangements[list, n] gives all permutations of length n from list (ordered subsets).",
        ),
        (
            "Array",
            "Array[f, n] generates {f[1], f[2], ..., f[n]}.\nArray[f, {n}] generates {f[1], f[2], ..., f[n]}.\nArray[f, {n, m}] generates {f[n], f[n+1], ..., f[m]}.",
        ),
        (
            "ArrayFlatten",
            "ArrayFlatten[{{m11, m12}, {m21, m22}}] flattens a matrix of matrices into a single matrix.",
        ),
        (
            "ArrayPad",
            "ArrayPad[list, n] pads list with n zeros on each side.\n\
             ArrayPad[list, {before, after}] pads with different amounts on each side.\n\
             ArrayPad[list, n, val] pads with val instead of 0.",
        ),
        (
            "ArrayReshape",
            "ArrayReshape[list, {d1, d2, ...}] reshapes a flat list into the given dimensions.\n\
             The total number of elements must match the product of dimensions.\n\
             Example: ArrayReshape[{1,2,3,4,5,6}, {2,3}] → {{1,2,3},{4,5,6}}.",
        ),
        (
            "AssociateTo",
            "AssociateTo[assoc, rule] returns a new association with the key->value added.\n\
             AssociateTo[assoc, {rule1, rule2, ...}] adds multiple entries.",
        ),
        (
            "AssociationQ",
            "AssociationQ[expr] returns True if expr is a valid association.",
        ),
        (
            "BaseForm",
            "BaseForm[expr, base] displays a number in the given base (2–36).",
        ),
        (
            "BellB",
            "BellB[n] gives the n-th Bell number.\nBellB[n, k] gives the partial Bell polynomial B_{n,k}.",
        ),
        (
            "BernoulliB",
            "BernoulliB[n] gives the n-th Bernoulli number B_n.",
        ),
        (
            "BesselSimplify",
            "BesselSimplify[expr] attempts to simplify Bessel function expressions.",
        ),
        (
            "BinCounts",
            "BinCounts[list, width] counts elements in bins of the given width.",
        ),
        (
            "Binarize",
            "Binarize[image] converts to black and white (threshold at 0.5).\n\
             Binarize[image, t] uses threshold t in [0,1].",
        ),
        (
            "Binomial",
            "Binomial[n, k] gives the binomial coefficient C(n, k).",
        ),
        (
            "Blend",
            "Blend[{c1, c2, ...}] averages a list of colors equally.\n\
             Blend[{c1, c2, ...}, {w1, w2, ...}] computes a weighted average.",
        ),
        (
            "BlockMap",
            "BlockMap[f, list, n] partitions list into non-overlapping blocks of size n and applies f to each.",
        ),
        (
            "Boole",
            "Boole[expr] returns 1 if expr is True, 0 otherwise.",
        ),
        (
            "BooleanQ",
            "BooleanQ[expr] returns True if expr is True or False, False otherwise.",
        ),
        (
            "Break",
            "Break[] exits the enclosing For, While, or Do loop.",
        ),
        (
            "Cases",
            "Cases[{e1, e2, ...}, pattern] gives a list of elements that match pattern.\n\
             Cases[list, pattern, levelspec] — not yet supported.",
        ),
        (
            "CatalanNumber",
            "CatalanNumber[n] gives the n-th Catalan number C_n = Binomial(2n, n) / (n+1).",
        ),
        (
            "Catch",
            "Catch[expr] evaluates expr, returning any value passed to Throw.",
        ),
        (
            "Ceiling",
            "Ceiling[x] gives the least integer greater than or equal to x.",
        ),
        (
            "CellInformation",
            "CellInformation[expr] returns cell information (notebook frontend not yet available).",
        ),
        (
            "CharacterCounts",
            "CharacterCounts[s] returns a list of {character, count} pairs for each distinct character in s.",
        ),
        (
            "Characters",
            "Characters[s] gives a list of the characters in string s.",
        ),
        (
            "ChineseRemainder",
            "ChineseRemainder[{a1, a2, ...}, {n1, n2, ...}] solves the system of congruences\n\
             x ≡ a_i (mod n_i) for pairwise coprime moduli.",
        ),
        (
            "Chop",
            "Chop[expr] replaces approximate real numbers close to 0 with exact 0.\n\
             Chop[expr, tol] uses tolerance tol (default 1e-10).",
        ),
        (
            "Clear",
            "Clear[symbol, ...] clears all definitions for the given symbols.",
        ),
        (
            "ClearAll",
            "ClearAll[symbol, ...] clears all definitions and attributes.",
        ),
        (
            "ClearSystemCache",
            "ClearSystemCache[] clears internal caches.",
        ),
        ("Close", "Close[stream] closes a stream."),
        (
            "ColorNegate",
            "ColorNegate[image] inverts the colors of an image.",
        ),
        (
            "Complement",
            "Complement[list1, list2, ...] gives elements in list1 not in any other list.",
        ),
        (
            "Complex",
            "Complex[a, b] constructs a complex number a + b I.",
        ),
        (
            "ComplexInfinity",
            "ComplexInfinity represents an infinite quantity on the complex plane.",
        ),
        (
            "Condition",
            "Condition[p, c] or p /; c matches p only if c is True.",
        ),
        (
            "ConnectedComponents",
            "ConnectedComponents[graph] gives the connected components of a graph.",
        ),
        (
            "ConstantArray",
            "ConstantArray[val, n] creates a list of n copies of val.",
        ),
        (
            "Continue",
            "Continue[] breaks to the next iteration in For, While, or Do.",
        ),
        ("CopyFile", "CopyFile[src, dest] copies a file."),
        ("Cos", "Cos[z] gives the cosine of z."),
        (
            "CosIntegral",
            "CosIntegral[z] gives the cosine integral Ci(z).",
        ),
        (
            "Count",
            "Count[list, pattern] returns the number of elements that match pattern.",
        ),
        (
            "Counts",
            "Counts[{e1, e2, ...}] returns an association with counts of each distinct element.",
        ),
        (
            "CreateDirectory",
            "CreateDirectory[dir] creates a new directory.",
        ),
        ("CreateFile", "CreateFile[name] creates a new empty file."),
        (
            "D",
            "D[f, x] gives the partial derivative of f with respect to x.\nD[f, x, y, ...] computes successive derivatives.",
        ),
        (
            "DSolve",
            "DSolve[eqn, y, x] solves a differential equation for y as a function of x.",
        ),
        (
            "Data",
            "Data[{k1 -> v1, k2 -> v2, ...}] creates a data expression.",
        ),
        (
            "Dataset",
            "Dataset[data] creates a structured data query object.",
        ),
        (
            "Date",
            "Date[] returns the current date as {year, month, day, hour, minute, second}.",
        ),
        (
            "DateDifference",
            "DateDifference[date1, date2] gives the difference in seconds.",
        ),
        (
            "DateList",
            "DateList[] returns the current date as a list.\nDateList[s] parses a date string.",
        ),
        (
            "DateMinusDays",
            "DateMinusDays[date, n] subtracts n days from date.",
        ),
        ("DatePlus", "DatePlus[date, n] adds n days to date."),
        ("DatePlusDays", "DatePlusDays[date, n] adds n days to date."),
        (
            "DateString",
            "DateString[] returns the current date as a string.\nDateString[format] returns a formatted date string.",
        ),
        (
            "Debug",
            "Debug[expr] starts the debugger on the evaluation of expr.",
        ),
        (
            "Definition",
            "Definition[symbol] shows the definitions of symbol.",
        ),
        (
            "Degree",
            "Degree is π/180, the conversion factor from degrees to radians.",
        ),
        (
            "Delete",
            "Delete[list, n] deletes the element at position n in list (1-indexed, negative counts from end).",
        ),
        (
            "DeleteCases",
            "DeleteCases[list, pattern] removes all matching elements.",
        ),
        (
            "DeleteDirectory",
            "DeleteDirectory[dir] deletes an empty directory.",
        ),
        (
            "DeleteDuplicates",
            "DeleteDuplicates[list] deletes all duplicates from list, keeping the first occurrence.",
        ),
        ("DeleteFile", "DeleteFile[name] deletes a file."),
        (
            "DeleteObject",
            "DeleteObject[obj] deletes an object and releases its resources.",
        ),
        (
            "Derivative",
            "Derivative[1][f] represents the first derivative of f.\nDerivative[n][f] represents the n-th derivative of f.",
        ),
        (
            "Differences",
            "Differences[list] computes the adjacent differences of list elements.",
        ),
        (
            "DigitCount",
            "DigitCount[n] counts decimal digits in n.\nDigitCount[n, b] counts digits in base b.",
        ),
        (
            "DirectedInfinity",
            "DirectedInfinity[] gives ComplexInfinity. DirectedInfinity[z] indicates an infinite quantity in direction z.",
        ),
        (
            "DirectoryQ",
            "DirectoryQ[filename] returns True if the path points to a directory.",
        ),
        ("Divide", "Divide[a, b] or a / b gives a divided by b."),
        (
            "DivideBy",
            "DivideBy[x, n] or x /= n divides x by n and stores the result.",
        ),
        (
            "DivisorSigma",
            "DivisorSigma[k, n] gives the sum of the k-th powers of divisors of n.",
        ),
        (
            "DivisorSum",
            "DivisorSum[n, f] gives the sum of f[d] over all positive divisors d of n.",
        ),
        (
            "Do",
            "Do[body, {n}] evaluates body n times.\nDo[body, {i, m, n}] evaluates body with i ranging from m to n.",
        ),
        (
            "Drop",
            "Drop[list, n] gives list with the first n elements removed.\n\
             Drop[list, -n] removes the last n elements.\n\
             Drop[list, {m, n}] removes elements m through n (inclusive).",
        ),
        (
            "E",
            "E is the base of the natural logarithm, approximately 2.71828.",
        ),
        ("EdgeCount", "EdgeCount[graph] gives the number of edges."),
        ("EdgeDetect", "EdgeDetect[image] detects edges in an image."),
        ("EdgeList", "EdgeList[graph] gives the list of edges."),
        (
            "Equal",
            "Equal[a, b] or a == b returns True if a and b are equal.",
        ),
        (
            "Equivalent",
            "Equivalent[a, b, ...] returns True if all arguments have the same truth value.\n\
             Equivalent[] = True.",
        ),
        ("Erf", "Erf[z] gives the error function erf(z)."),
        (
            "Erfc",
            "Erfc[z] gives the complementary error function (1 - erf(z)).",
        ),
        (
            "Erfi",
            "Erfi[z] = Erf[i z] / i gives the imaginary error function.",
        ),
        (
            "EulerGamma",
            "EulerGamma ≈ 0.5772156649... (Euler-Mascheroni constant).",
        ),
        (
            "EulerPhi",
            "EulerPhi[n] gives the Euler totient function φ(n).",
        ),
        (
            "Evaluate",
            "Evaluate[expr] forces immediate evaluation of expr.",
        ),
        (
            "EvaluateKernel",
            "EvaluateKernel[expr] evaluates expr on a remote kernel (not yet supported).",
        ),
        ("EvenQ", "EvenQ[n] returns True if n is an even integer."),
        (
            "ExactNumberQ",
            "ExactNumberQ[expr] returns True if expr is an exact number.",
        ),
        (
            "Expand",
            "Expand[expr] expands out products and powers in expr.",
        ),
        (
            "ExpIntegralE",
            "ExpIntegralE[n, z] gives the exponential integral E_n(z).",
        ),
        (
            "ExpIntegralEi",
            "ExpIntegralEi[z] gives the exponential integral Ei(z).",
        ),
        (
            "Export",
            "Export[filename, expr] exports expr to the specified file.",
        ),
        (
            "FFT",
            "FFT[list] computes the discrete Fourier transform of a list.",
        ),
        (
            "Factor",
            "Factor[expr] factors a polynomial over the integers.",
        ),
        ("Factorial", "Factorial[n] or n! gives the factorial of n."),
        (
            "Factorial2",
            "Factorial2[n] or n!! gives the double factorial of n.",
        ),
        (
            "FactorialPower",
            "FactorialPower[n, k] gives the falling factorial n*(n-1)*...*(n-k+1).",
        ),
        ("Fibonacci", "Fibonacci[n] gives the n-th Fibonacci number."),
        (
            "Field",
            "Field[obj, name] accesses a field on an object.\nobj[field] is syntactic sugar for field access.",
        ),
        (
            "FileDate",
            "FileDate[filename] returns the modification date of a file.",
        ),
        (
            "FileExistsQ",
            "FileExistsQ[filename] returns True if the file exists.",
        ),
        (
            "FileNames",
            "FileNames[pattern] lists files matching pattern.",
        ),
        (
            "FileSize",
            "FileSize[filename] returns the size of a file in bytes.",
        ),
        (
            "FindMinimum",
            "FindMinimum[f, {x, x0}] finds a local minimum of f near x0.",
        ),
        (
            "FindRoot",
            "FindRoot[f, {x, x0}] searches for a numerical root of f near x0.",
        ),
        (
            "FindShortestPath",
            "FindShortestPath[graph, start, end] finds the shortest path between vertices.",
        ),
        ("First", "First[expr] gives the first element of expr."),
        (
            "FirstPosition",
            "FirstPosition[list, pattern] gives the position of the first element matching pattern.",
        ),
        (
            "FixedPoint",
            "FixedPoint[f, expr] applies f repeatedly until the result no longer changes.",
        ),
        (
            "Flatten",
            "Flatten[expr] flattens nested lists into a single list.",
        ),
        (
            "Floor",
            "Floor[x] gives the greatest integer less than or equal to x.",
        ),
        (
            "Fold",
            "Fold[f, init, list] gives the last element of FoldList[f, init, list].",
        ),
        (
            "FoldList",
            "FoldList[f, init, list] gives a list of successive fold results from the left.\n\
             FoldList[f, init, list] gives all intermediate results of folding f from the left.",
        ),
        (
            "For",
            "For[start, test, inc, body] evaluates start, then repeats test, body, inc.",
        ),
        (
            "FractionalPart",
            "FractionalPart[x] gives the fractional part of x.",
        ),
        (
            "FreeQ",
            "FreeQ[expr, pattern] returns True if no part of expr matches pattern.",
        ),
        ("FresnelC", "FresnelC[z] gives the Fresnel integral C(z)."),
        ("FresnelS", "FresnelS[z] gives the Fresnel integral S(z)."),
        (
            "FromCharacterCode",
            "FromCharacterCode[n] converts an integer character code to a string.",
        ),
        (
            "FromDigits",
            "FromDigits[digits] constructs an integer from its decimal digits.",
        ),
        (
            "FullDefinition",
            "FullDefinition[symbol] shows complete definitions.",
        ),
        ("GCD", "GCD[a, b, ...] gives the greatest common divisor."),
        (
            "Gamma",
            "Gamma[z] gives the gamma function Γ(z).\nGamma[n] gives (n-1)! for integer n.",
        ),
        (
            "GatherBy",
            "GatherBy[list, f] groups elements by the values of f applied to each element.",
        ),
        (
            "GegenbauerC",
            "GegenbauerC[n, m, x] gives the Gegenbauer polynomial C_n^(m)(x).",
        ),
        (
            "Globals",
            "Globals[] returns a list of all user-defined symbols.",
        ),
        ("GoldenRatio", "GoldenRatio ≈ 1.6180339887..."),
        (
            "Graph",
            "Graph[{a, b, ...}, ...] constructs a graph (not yet fully implemented).",
        ),
        (
            "GraphPlot",
            "GraphPlot[edges] displays a graphical plot of the graph using ASCII output.",
        ),
        (
            "Greater",
            "Greater[a, b] or a > b returns True if a is strictly greater than b.",
        ),
        (
            "GreaterEqual",
            "GreaterEqual[a, b] or a >= b returns True if a is greater than or equal to b.",
        ),
        (
            "GroupBy",
            "GroupBy[list, f] groups elements by the values of f applied to each element.",
        ),
        (
            "HankelMatrix",
            "HankelMatrix[{c1, c2, ..., cn}] constructs the n×n Hankel matrix with first column c and last row r.",
        ),
        (
            "HarmonicNumber",
            "HarmonicNumber[n] gives the n-th harmonic number H_n.",
        ),
        ("Head", "Head[expr] gives the head of expr."),
        (
            "Help",
            "Help[symbol] or ? symbol shows usage information for symbol.",
        ),
        (
            "HermiteH",
            "HermiteH[n, x] gives the Hermite polynomial H_n(x).",
        ),
        (
            "HighpassFilter",
            "HighpassFilter[list, cutoff] applies a highpass filter.",
        ),
        (
            "HoldForm",
            "HoldForm[expr] displays expr without evaluating.",
        ),
        (
            "Hypergeometric1F1",
            "Hypergeometric1F1[a, b, z] is Kummer's confluent hypergeometric function.",
        ),
        (
            "Hypergeometric2F1",
            "Hypergeometric2F1[a, b, c, z] is the Gauss hypergeometric function.",
        ),
        ("I", "I is the imaginary unit √(-1)."),
        (
            "Image",
            "Image[data] constructs an image from numeric data.",
        ),
        (
            "ImageAdjust",
            "ImageAdjust[image] adjusts the brightness and contrast of an image.",
        ),
        (
            "ImageCrop",
            "ImageCrop[image, size] crops an image to the given size.",
        ),
        (
            "ImageData",
            "ImageData[image] gives the pixel data of an image as a list.",
        ),
        (
            "ImageDimensions",
            "ImageDimensions[image] gives the dimensions of an image.",
        ),
        (
            "ImageResize",
            "ImageResize[image, size] resizes an image to the given dimensions.",
        ),
        (
            "ImageRotate",
            "ImageRotate[image, angle] rotates an image by the given angle.",
        ),
        (
            "ImageType",
            "ImageType[image] gives the type of image data: \"Real\", \"Byte\", or \"Bit\".",
        ),
        (
            "Implies",
            "Implies[p, q] returns True unless p is True and q is False (p → q).",
        ),
        (
            "Import",
            "Import[filename] imports data from the specified file.",
        ),
        (
            "Indeterminate",
            "Indeterminate represents an indeterminate expression (e.g., 0/0).",
        ),
        (
            "InexactNumberQ",
            "InexactNumberQ[expr] returns True if expr is an approximate (floating-point) number.",
        ),
        ("Infinity", "Infinity represents an infinite quantity."),
        (
            "Information",
            "Information[symbol] shows information about symbol.",
        ),
        ("Input", "Input[] reads a line from stdin as an expression."),
        (
            "InputString",
            "InputString[] reads a line from stdin as a string.",
        ),
        (
            "Insert",
            "Insert[list, elem, n] inserts elem at position n in list (1-indexed, negative counts from end).",
        ),
        (
            "Integer",
            "Integer[n] coerces n to an integer (truncates real numbers).",
        ),
        (
            "IntegerDigits",
            "IntegerDigits[n] gives the decimal digits of n.\nIntegerDigits[n, b] gives digits in base b.",
        ),
        (
            "IntegerLength",
            "IntegerLength[n] gives the number of digits of n in base 10.\nIntegerLength[n, b] gives the number of digits in base b.",
        ),
        ("IntegerPart", "IntegerPart[x] gives the integer part of x."),
        (
            "IntegerQ",
            "IntegerQ[expr] returns True if expr is an integer.",
        ),
        (
            "IntegerString",
            "IntegerString[n] gives the decimal string of n.\nIntegerString[n, b] gives the base-b representation.",
        ),
        (
            "Integrate",
            "Integrate[f, x] gives the indefinite integral of f with respect to x.\n\
             Integrate[f, {x, a, b}] gives the definite integral from a to b.",
        ),
        (
            "Intersection",
            "Intersection[list1, list2, ...] gives common elements shared by all lists.",
        ),
        (
            "InverseFFT",
            "InverseFFT[list] computes the inverse Fourier transform.",
        ),
        (
            "JacobiP",
            "JacobiP[n, a, b, x] gives the Jacobi polynomial P_n^(a,b)(x).",
        ),
        ("Join", "Join[list1, list2, ...] concatenates lists."),
        (
            "KelvinBei",
            "KelvinBei[z] gives the Kelvin function bei(z).",
        ),
        (
            "KelvinBer",
            "KelvinBer[z] gives the Kelvin function ber(z).",
        ),
        (
            "KelvinKei",
            "KelvinKei[z] gives the Kelvin function kei(z).",
        ),
        (
            "KelvinKer",
            "KelvinKer[z] gives the Kelvin function ker(z).",
        ),
        (
            "KeyExistsQ",
            "KeyExistsQ[assoc, key] returns True if key is present in the association.",
        ),
        (
            "KeySort",
            "KeySort[assoc] returns the association sorted by keys.",
        ),
        (
            "Keys",
            "Keys[assoc] returns the list of keys in the association.",
        ),
        ("LCM", "LCM[a, b, ...] gives the least common multiple."),
        (
            "LaguerreL",
            "LaguerreL[n, x] gives the Laguerre polynomial L_n(x).\n\
             LaguerreL[n, k, x] gives the associated Laguerre polynomial.",
        ),
        ("Last", "Last[expr] gives the last element of expr."),
        (
            "LegendreP",
            "LegendreP[n, x] gives the Legendre polynomial P_n(x).",
        ),
        (
            "LegendreQ",
            "LegendreQ[n, x] gives the Legendre function of the second kind Q_n(x).",
        ),
        (
            "Length",
            "Length[expr] gives the number of elements in expr.",
        ),
        (
            "Less",
            "Less[a, b] or a < b returns True if a is strictly less than b.",
        ),
        (
            "LessEqual",
            "LessEqual[a, b] or a <= b returns True if a is less than or equal to b.",
        ),
        (
            "LinearProgramming",
            "LinearProgramming[c, m, b] solves the linear programming problem: minimize c·x subject to m·x ≥ b.",
        ),
        (
            "ListLinePlot",
            "ListLinePlot[data] generates a line plot of a list of points.",
        ),
        (
            "ListPlot",
            "ListPlot[data] generates a plot of a list of points.",
        ),
        ("ListQ", "ListQ[expr] returns True if expr is a list."),
        (
            "Log",
            "Log[z] gives the natural logarithm of z.\nLog[a, z] gives the logarithm of z to base a.",
        ),
        ("Log10", "Log10[z] gives the base-10 logarithm of z."),
        ("Log2", "Log2[z] gives the base-2 logarithm of z."),
        (
            "LogGamma",
            "LogGamma[z] gives the logarithm of the gamma function.",
        ),
        (
            "LogIntegral",
            "LogIntegral[z] gives the logarithmic integral li(z).\n\
             LogIntegral[z] = integral from 0 to z of dt / log(t).",
        ),
        (
            "LogisticSigmoid",
            "LogisticSigmoid[x] returns 1/(1+exp(-x)).",
        ),
        (
            "Lookup",
            "Lookup[assoc, key] returns the value associated with key in the association.",
        ),
        (
            "LowpassFilter",
            "LowpassFilter[list, cutoff] applies a lowpass filter.",
        ),
        (
            "MachineNumberQ",
            "MachineNumberQ[expr] returns True if expr is a machine-precision number.",
        ),
        (
            "Majority",
            "Majority[a, b, c, ...] returns True if more than half of the arguments are True.\n\
             Requires an odd number of arguments.",
        ),
        (
            "Map",
            "Map[f, expr] or f /@ expr applies f to each element at level 1 of expr.",
        ),
        (
            "MapApply",
            "MapApply[f, expr] (f @@@ expr) replaces heads at level 1, using elements of lists as arguments.\n\
             MapApply[f, {{a,b}, {c,d}}] → {f[a,b], f[c,d]}.",
        ),
        (
            "MatchQ",
            "MatchQ[expr, pattern] returns True if expr matches the pattern.",
        ),
        ("Max", "Max[a, b, ...] gives the maximum of its arguments."),
        (
            "MaxMemoryUsed",
            "MaxMemoryUsed[] gives the peak memory usage in bytes.",
        ),
        (
            "Maximize",
            "Maximize[expr, {x, y, ...}] symbolically maximizes a function.",
        ),
        (
            "MemberQ",
            "MemberQ[list, pattern] returns True if any element of list matches pattern.",
        ),
        (
            "MemoryConstrained",
            "MemoryConstrained[expr, limit] evaluates expr, aborting if memory usage exceeds limit (in bytes).",
        ),
        (
            "MemoryInUse",
            "MemoryInUse[] gives the current memory usage in bytes.",
        ),
        (
            "Method",
            "Method[obj, name, args...] calls a method on an object.\n\
             obj @ name[args...] is syntactic sugar for method calls.",
        ),
        (
            "MethodCall",
            "MethodCall displays a method call in standard form.",
        ),
        ("Min", "Min[a, b, ...] gives the minimum of its arguments."),
        (
            "Minimize",
            "Minimize[expr, {x, y, ...}] symbolically minimizes a function.",
        ),
        ("Minus", "Minus[x] or -x gives the negation of x."),
        ("Mod", "Mod[a, b] gives a mod b."),
        ("MoebiusMu", "MoebiusMu[n] gives the Möbius function μ(n)."),
        (
            "Most",
            "Most[expr] gives expr with the last element removed.",
        ),
        (
            "MovingAverage",
            "MovingAverage[list, n] computes the moving average of list with window size n.",
        ),
        (
            "MovingMedian",
            "MovingMedian[list, n] computes the moving median of list with window size n.",
        ),
        (
            "MultiplicativeOrder",
            "MultiplicativeOrder[a, n] gives the smallest integer k such that a^k ≡ 1 (mod n).",
        ),
        (
            "N",
            "N[expr] gives the numerical value of expr.\nN[expr, n] uses n decimal digits of precision.",
        ),
        (
            "NMaximize",
            "NMaximize[f, {x, y, ...}] numerically maximizes a function.",
        ),
        (
            "NMinimize",
            "NMinimize[f, {x, y, ...}] numerically minimizes a function (Nelder-Mead).",
        ),
        (
            "NRoots",
            "NRoots[expr, x] gives approximate roots of a polynomial in x.",
        ),
        (
            "Names",
            "Names[] gives a list of all defined symbols.\nNames[pattern] gives symbols matching a pattern.",
        ),
        (
            "Nand",
            "Nand[a, b, ...] returns False if all arguments are True, True otherwise.\n\
             Nand[] = False.",
        ),
        ("Nest", "Nest[f, expr, n] applies f to expr n times."),
        (
            "NestList",
            "NestList[f, expr, n] gives a list of the results of applying f to expr 0 through n times.",
        ),
        (
            "NestWhile",
            "NestWhile[f, expr, test] applies f repeatedly as long as test is True.",
        ),
        (
            "NestWhileList",
            "NestWhileList[f, expr, test] returns all intermediate results as long as test is True.",
        ),
        (
            "New",
            "New[class, args...] creates a new object of the given class.",
        ),
        (
            "Nor",
            "Nor[a, b, ...] returns True if no argument is True, False otherwise.\n\
             Nor[] = True.",
        ),
        (
            "Not",
            "Not[expr] or !expr returns the logical negation of expr.",
        ),
        ("Nothing", "Nothing is automatically removed from lists."),
        (
            "NumericQ",
            "NumericQ[expr] returns True if expr is a numeric quantity.",
        ),
        (
            "NumericStringQ",
            "NumericStringQ[s] returns True if s is a string that represents a number.",
        ),
        (
            "Object",
            "Object[class, fields] creates a lightweight object.",
        ),
        (
            "ObjectQ",
            "ObjectQ[expr] returns True if expr is an object.",
        ),
        ("OddQ", "OddQ[n] returns True if n is an odd integer."),
        (
            "OpenAppend",
            "OpenAppend[filename] opens a file for appending.",
        ),
        ("OpenRead", "OpenRead[filename] opens a file for reading."),
        ("OpenWrite", "OpenWrite[filename] opens a file for writing."),
        (
            "Options",
            "Options[symbol] gives the list of default options for symbol.",
        ),
        (
            "OptionsPattern",
            "OptionsPattern[] matches a sequence of options in a function definition.",
        ),
        (
            "Or",
            "Or[a, b, ...] or a || b || ... evaluates arguments left to right, returning \
             the first value that is True, or the last value if none are True.\n\
             Or[] = False.",
        ),
        (
            "Ordering",
            "Ordering[list] returns the positions that would sort list.\n\
             Ordering[list, n] returns the first n positions.\n\
             Ordering[list, -n] returns the last n positions.",
        ),
        (
            "PadLeft",
            "PadLeft[list, n] pads list on the left to length n with zeros.\n\
             PadLeft[list, n, x] pads list on the left with x to length n.",
        ),
        (
            "PadRight",
            "PadRight[list, n] pads list on the right to length n with zeros.\n\
             PadRight[list, n, x] pads list on the right with x to length n.",
        ),
        (
            "Part",
            "Part[expr, i] or expr[[i]] gives the i-th part of expr.",
        ),
        (
            "Partition",
            "Partition[list, n] splits list into sublists of length n.\n\
             Partition[list, n, d] uses offset d between successive sublists.",
        ),
        (
            "PartitionsP",
            "PartitionsP[n] gives the number of integer partitions of n.",
        ),
        (
            "Pathfinding",
            "Pathfinding[graph, start, end] finds the shortest path between vertices.",
        ),
        (
            "Pattern",
            "Pattern[head, match] is the internal representation of a named pattern.",
        ),
        ("Pi", "Pi is π ≈ 3.1415926535..."),
        (
            "Plot",
            "Plot[f, {x, min, max}] generates a plot of f as x ranges from min to max.",
        ),
        (
            "Plus",
            "Plus[a, b, ...] or a + b + ... computes the sum of its arguments.",
        ),
        (
            "PolyGamma",
            "PolyGamma[n, z] gives the n-th derivative of digamma function ψ(z).",
        ),
        (
            "Position",
            "Position[list, pattern] returns the positions of elements that match pattern.",
        ),
        (
            "PostDecrement",
            "PostDecrement[x] or x-- returns x, then decrements it by 1.",
        ),
        (
            "PostIncrement",
            "PostIncrement[x] or x++ returns x, then increments it by 1.",
        ),
        (
            "Power",
            "Power[a, b] or a ^ b gives a raised to the power b.",
        ),
        (
            "PowerMod",
            "PowerMod[a, b, n] computes a^b mod n efficiently.",
        ),
        (
            "PreDecrement",
            "PreDecrement[x] or --x decrements x by 1 and returns the new value.",
        ),
        (
            "PreIncrement",
            "PreIncrement[x] or ++x increments x by 1 and returns the new value.",
        ),
        (
            "Prepend",
            "Prepend[expr, elem] returns expr with elem prepended.",
        ),
        (
            "PrependTo",
            "PrependTo[x, elem] prepends elem to x and stores the result.",
        ),
        ("Prime", "Prime[n] gives the n-th prime number."),
        ("PrimePi", "PrimePi[x] gives the number of primes ≤ x."),
        ("PrimeQ", "PrimeQ[n] returns True if n is prime."),
        (
            "Print",
            "Print[expr1, expr2, ...] prints the expressions to stdout.",
        ),
        (
            "PrintF",
            "PrintF[fmt, args...] prints a formatted string (Rust-style formatting).",
        ),
        (
            "Profile",
            "Profile[expr] evaluates expr and shows profiling information.",
        ),
        (
            "Protect",
            "Protect[symbol, ...] protects symbols from being modified.",
        ),
        (
            "QuantumEntangle",
            "QuantumEntangle[state1, state2] produces a tensor product of two states.",
        ),
        (
            "QuantumMeasure",
            "QuantumMeasure[state] simulates a quantum measurement on a state.",
        ),
        (
            "QuantumOperator",
            "QuantumOperator[matrix] creates a quantum operator (gate) from a matrix.",
        ),
        (
            "QuantumState",
            "QuantumState[amplitudes] creates a quantum state from a list of amplitudes.",
        ),
        (
            "QuantumTensorProduct",
            "QuantumTensorProduct[states] produces a tensor product of states.",
        ),
        (
            "QuoteCharacter",
            "QuoteCharacter[s] wraps s with double quotes and escapes internal quotes.",
        ),
        (
            "RandomChoice",
            "RandomChoice[list] picks a random element from list.",
        ),
        (
            "RandomInteger",
            "RandomInteger[{min, max}] returns a random integer in the range.",
        ),
        (
            "RandomPermutation",
            "RandomPermutation[n] generates a random permutation of {1, 2, ..., n}.",
        ),
        (
            "RandomReal",
            "RandomReal[{min, max}] returns a random real number in the range.",
        ),
        (
            "RandomSample",
            "RandomSample[list] gives a random permutation of list.\n\
             RandomSample[list, n] gives n elements without replacement.",
        ),
        (
            "Range",
            "Range[n] gives {1, 2, ..., n}.\n\
             Range[min, max] gives {min, min+1, ..., max}.\n\
             Range[min, max, step] uses the given step.",
        ),
        (
            "Rational",
            "Rational[a, b] or a / b coerces an exact rational number.",
        ),
        (
            "Rationalize",
            "Rationalize[x] converts a real number to a rational approximation.",
        ),
        ("Read", "Read[stream] reads an expression from a stream."),
        ("ReadLine", "ReadLine[stream] reads a line from a stream."),
        (
            "ReadString",
            "ReadString[filename] reads the contents of a file as a string.",
        ),
        ("Real", "Real[n] coerces n to a real number."),
        (
            "RealAbs",
            "RealAbs[x] returns the absolute value of a real number x.",
        ),
        (
            "RealDigits",
            "RealDigits[x] gives a list of digits and exponent for the real number x.",
        ),
        ("ReleaseHold", "ReleaseHold[expr] removes Hold from expr."),
        ("Remove", "Remove[symbol, ...] removes symbols entirely."),
        ("RenameFile", "RenameFile[src, dest] renames a file."),
        (
            "Replace",
            "Replace[expr, rules] applies rules to the whole expression.",
        ),
        (
            "ReplaceAll",
            "ReplaceAll[expr, rules] or expr /. rules applies rules once.",
        ),
        (
            "ReplacePart",
            "ReplacePart[list, n, new] replaces the element at position n in list with new.",
        ),
        (
            "ReplaceRepeated",
            "ReplaceRepeated[expr, rules] or expr //. rules applies rules until no change.",
        ),
        (
            "Rest",
            "Rest[expr] gives expr with the first element removed.",
        ),
        (
            "Reverse",
            "Reverse[expr] reverses the order of elements in expr.",
        ),
        (
            "Riffle",
            "Riffle[list, x] inserts x between consecutive elements of list.",
        ),
        (
            "RotateLeft",
            "RotateLeft[list, n] rotates the elements of list n positions to the left.",
        ),
        (
            "RotateRight",
            "RotateRight[list, n] rotates the elements of list n positions to the right.",
        ),
        (
            "Round",
            "Round[x] gives the integer nearest to x, rounding ties away from 0.",
        ),
        (
            "Rule",
            "Rule[a, b] or a -> b represents a rule replacing a with b.",
        ),
        (
            "RuleDelayed",
            "RuleDelayed[a, b] or a :> b is a delayed rule (right side unevaluated).",
        ),
        (
            "SameQ",
            "SameQ[a, b] or a === b returns True if a and b are identical.",
        ),
        ("Scan", "Scan[f, expr] applies f to each element of expr."),
        (
            "Select",
            "Select[list, crit] picks elements of list for which crit returns True.",
        ),
        (
            "Sequence",
            "Sequence[a, b, ...] splices arguments into a function.",
        ),
        (
            "Series",
            "Series[f, {x, x0, n}] gives the power series expansion of f about x0 to order n.",
        ),
        ("Set", "Set[lhs, rhs] sets lhs to rhs."),
        (
            "SetAttributes",
            "SetAttributes[symbol, attr] sets attributes for a symbol.",
        ),
        (
            "SetDelayed",
            "SetDelayed[lhs, rhs] or lhs := rhs sets lhs to rhs (evaluated when used).",
        ),
        (
            "SetField",
            "SetField[obj, name, value] sets a field on an object.\nobj[field] = value is syntactic sugar.",
        ),
        (
            "SetPart",
            "SetPart[lhs, i, val] or lhs[[i]] = val sets the i-th part of lhs.",
        ),
        (
            "Signature",
            "Signature[perm] gives the sign of a permutation (+1/-1).",
        ),
        ("Simplify", "Simplify[expr] simplifies expr algebraically."),
        ("Sin", "Sin[z] gives the sine of z."),
        (
            "SinIntegral",
            "SinIntegral[z] gives the sine integral Si(z).",
        ),
        (
            "SinhIntegral",
            "SinhIntegral[z] gives the hyperbolic sine integral Shi(z).",
        ),
        (
            "Solve",
            "Solve[expr, var] attempts to solve an equation for var.\n\
             Solve[{eq1, eq2, ...}, {var1, var2, ...}] solves a system.",
        ),
        ("Sort", "Sort[list] sorts list in the default order."),
        (
            "Spectrogram",
            "Spectrogram[audio] generates a spectrogram of an audio signal.",
        ),
        (
            "SphericalBesselJ",
            "SphericalBesselJ[n, z] gives the spherical Bessel function j_n(z).",
        ),
        (
            "SphericalBesselY",
            "SphericalBesselY[n, z] gives the spherical Bessel function y_n(z).",
        ),
        (
            "Split",
            "Split[list] splits list into runs of identical adjacent elements.",
        ),
        (
            "SplitBy",
            "SplitBy[list, f] splits list into runs where f applied to each element gives identical values.",
        ),
        ("Sqrt", "Sqrt[z] or Sqrt[z] gives the square root of z."),
        (
            "StirlingS1",
            "StirlingS1[n, k] gives the Stirling number of the first kind.",
        ),
        (
            "StirlingS2",
            "StirlingS2[n, k] gives the Stirling number of the second kind.",
        ),
        (
            "StringBlock",
            "StringBlock[\"line1\", \"line2\", ...] joins strings with newlines.",
        ),
        (
            "StringCases",
            "StringCases[s, pat] gives all substrings of s that match pat.",
        ),
        (
            "StringContainsQ",
            "StringContainsQ[s, sub] returns True if s contains sub as a substring.",
        ),
        (
            "StringCount",
            "StringCount[s, pat] counts substrings of s that match pat.",
        ),
        (
            "StringDelete",
            "StringDelete[s, pat] deletes substrings matching pat.",
        ),
        (
            "StringDrop",
            "StringDrop[s, n] drops the first n characters of s.",
        ),
        (
            "StringEndsQ",
            "StringEndsQ[s, sub] returns True if s ends with sub.",
        ),
        (
            "StringExtract",
            "StringExtract[s, n] extracts the n-th character from s.\n\
             StringExtract[s, {m, n}] extracts characters m through n.",
        ),
        (
            "StringInsert",
            "StringInsert[s, new, n] inserts new into string s at position n.\n\
             Negative positions count from the end.",
        ),
        (
            "StringJoin",
            "StringJoin[s1, s2, ...] or s1 <> s2 <> ... concatenates strings.",
        ),
        (
            "StringJoinConstant",
            "StringJoinConstant[s, n] concatenates n copies of string s.",
        ),
        (
            "StringLength",
            "StringLength[s] gives the number of characters in string s.",
        ),
        (
            "StringMatchQ",
            "StringMatchQ[s, pattern] returns True if the whole string matches the pattern.",
        ),
        (
            "StringPadLeft",
            "StringPadLeft[s, n] pads string s on the left with spaces to length n.",
        ),
        (
            "StringPadRight",
            "StringPadRight[s, n] pads string s on the right with spaces to length n.",
        ),
        (
            "StringPart",
            "StringPart[s, n] gives the n-th character in string s.",
        ),
        (
            "StringPosition",
            "StringPosition[s, pat] gives the start and end positions of all matches of pat in s.",
        ),
        (
            "StringRepeat",
            "StringRepeat[s, n] repeats string s n times.",
        ),
        (
            "StringReplace",
            "StringReplace[s, old -> new] replaces substrings of s.",
        ),
        (
            "StringReplacePart",
            "StringReplacePart[s, new, {m, n}] replaces characters m through n with new.",
        ),
        (
            "StringReverse",
            "StringReverse[s] reverses the characters in string s.",
        ),
        (
            "StringRiffle",
            "StringRiffle[{s1, s2, ...}] concatenates strings with spaces between them.",
        ),
        (
            "StringSplit",
            "StringSplit[s] splits a string at whitespace into a list of substrings.",
        ),
        (
            "StringStartsQ",
            "StringStartsQ[s, sub] returns True if s starts with sub.",
        ),
        (
            "StringTake",
            "StringTake[s, n] gives the first n characters of s.",
        ),
        (
            "StringToByteArray",
            "StringToByteArray[s] converts a string to a byte array (UTF-8 bytes).",
        ),
        (
            "StringTrim",
            "StringTrim[s] removes leading and trailing whitespace from s.",
        ),
        ("StruveH", "StruveH[n, z] gives the Struve function H_n(z)."),
        (
            "StruveL",
            "StruveL[n, z] gives the modified Struve function L_n(z).",
        ),
        (
            "Subfactorial",
            "Subfactorial[n] gives the number of derangements of n objects.",
        ),
        (
            "SubtractFrom",
            "SubtractFrom[x, n] or x -= n subtracts n from x.",
        ),
        (
            "Symmetrize",
            "Symmetrize[tensor] symmetrizes a tensor over all indices.\n\
             Symmetrize[tensor, {i, j}] symmetrizes over indices i and j.",
        ),
        (
            "SystemDialog",
            "SystemDialog[title, message] displays a simple system dialog box.",
        ),
        (
            "Table",
            "Table[expr, n] generates a list of n copies of expr.\n\
             Table[expr, {i, max}] evaluates expr for i from 1 to max.\n\
             Table[expr, {i, min, max}] evaluates expr for i from min to max.\n\
             Table[expr, {i, min, max, step}] uses the given step.\n\
             Table[expr, {i, {val1, val2, ...}}] uses successive values from the list.\n\
             Table[expr, {i, imin, imax}, {j, jmin, jmax}, ...] gives a nested list.",
        ),
        ("Take", "Take[list, n] gives the first n elements of list."),
        (
            "Tally",
            "Tally[list] counts occurrences of each distinct element in list.",
        ),
        ("Tan", "Tan[z] gives the tangent of z."),
        (
            "TensorContract",
            "TensorContract[tensor, {i, j}] contracts indices i and j of a tensor.\n\
             TensorContract[tensor, {{i1,j1}, {i2,j2}, ...}] does multiple contractions.",
        ),
        (
            "TensorDimensions",
            "TensorDimensions[tensor] gives the dimensions of a tensor.\n\
             Returns {} for scalars, {n} for vectors, {n,m} for matrices, etc.",
        ),
        (
            "TensorProduct",
            "TensorProduct[t1, t2, ...] gives the tensor product (Kronecker product).",
        ),
        (
            "TensorRank",
            "TensorRank[tensor] gives the rank of a tensor.",
        ),
        (
            "TensorTranspose",
            "TensorTranspose[tensor] transposes the first two indices.\n\
             TensorTranspose[tensor, perm] transposes according to permutation perm.",
        ),
        (
            "Thread",
            "Thread[f[args]] threads f over lists in args.\n\
             Thread[operator, List] is implicit when operator is applied to lists.",
        ),
        ("Through", "Through[p[f, g][x]] gives p[f[x], g[x]]."),
        (
            "Throw",
            "Throw[value] throws a value that can be caught by Catch.",
        ),
        (
            "TimeConstrained",
            "TimeConstrained[expr, t] evaluates expr, aborting after t seconds.",
        ),
        (
            "TimeRemaining",
            "TimeRemaining[] gives the remaining evaluation time (not yet implemented).",
        ),
        (
            "Times",
            "Times[a, b, ...] or a * b * ... computes the product of its arguments.",
        ),
        (
            "TimesBy",
            "TimesBy[x, n] or x *= n multiplies x by n and stores the result.",
        ),
        (
            "Timing",
            "Timing[expr] returns {time, result} where time is the wall-clock time in seconds.",
        ),
        (
            "ToCharacterCode",
            "ToCharacterCode[s] converts a string to a list of integer character codes.",
        ),
        (
            "ToExpression",
            "ToExpression[str] converts a string into an expression.",
        ),
        (
            "ToLowerCase",
            "ToLowerCase[s] converts all characters in s to lowercase.",
        ),
        ("ToString", "ToString[expr] converts expr to a string."),
        (
            "ToUpperCase",
            "ToUpperCase[s] converts all characters in s to uppercase.",
        ),
        (
            "Total",
            "Total[list] gives the total of all elements in list.",
        ),
        (
            "Trace",
            "Trace[expr] gives a list of all intermediate evaluations.",
        ),
        (
            "Transpose",
            "Transpose[list] transposes rows and columns in a matrix (list of lists).",
        ),
        (
            "TreePlot",
            "TreePlot[edges] displays a tree plot using ASCII output.",
        ),
        (
            "TrueQ",
            "TrueQ[expr] returns True if expr is the symbol True, otherwise False.",
        ),
        (
            "TypeOf",
            "TypeOf[expr] gives the internal type tag of expr as a string.",
        ),
        (
            "Unequal",
            "Unequal[a, b] or a != b returns True if a and b are not equal.",
        ),
        (
            "Unevaluated",
            "Unevaluated[expr] prevents evaluation of expr.",
        ),
        (
            "Union",
            "Union[list1, list2, ...] joins and sorts lists, removing duplicates.",
        ),
        (
            "UnitBox",
            "UnitBox[x] returns 1 if |x| < 1/2, 1/2 if |x| == 1/2, 0 otherwise.",
        ),
        ("Unitize", "Unitize[x] returns 0 if x == 0, 1 otherwise."),
        (
            "Unprotect",
            "Unprotect[symbol, ...] removes protection from symbols.",
        ),
        (
            "UnsameQ",
            "UnsameQ[a, b] or a =!= b returns True if a and b are not identical.",
        ),
        ("Unset", "Unset[lhs] removes any definition for lhs."),
        (
            "ValueQ",
            "ValueQ[expr] returns True if expr has a value defined.",
        ),
        (
            "Values",
            "Values[assoc] returns the list of values in the association.",
        ),
        (
            "VertexCount",
            "VertexCount[graph] gives the number of vertices.",
        ),
        (
            "VertexList",
            "VertexList[graph] gives the list of vertices.",
        ),
        (
            "WeatherData",
            "WeatherData[city] returns weather data for a given city.",
        ),
        (
            "WeatherForecast",
            "WeatherForecast[city, n] returns a weather forecast for n days.",
        ),
        (
            "WeatherHistory",
            "WeatherHistory[city, date] returns historical weather for a given date.",
        ),
        (
            "WeaklyConnectedComponents",
            "WeaklyConnectedComponents[graph] gives the weakly connected components.",
        ),
        (
            "While",
            "While[test, body] evaluates test, then body while test is True.",
        ),
        (
            "Write",
            "Write[filename, expr1, expr2, ...] writes expressions to a file, separated by newlines.",
        ),
        (
            "WriteLine",
            "WriteLine[filename, expr] writes expr to a file terminated by a newline.",
        ),
        (
            "WriteString",
            "WriteString[filename, str] writes str to a file.",
        ),
        (
            "Xor",
            "Xor[a, b, ...] returns True if an odd number of arguments are True.\n\
             Xor[] = False.",
        ),
        (
            "ZernikeR",
            "ZernikeR[n, m, r] gives the Zernike radial polynomial R_n^m(r).",
        ),
        ("Zeta", "Zeta[s] gives the Riemann zeta function ζ(s)."),
    ];

    entries
        .binary_search_by(|(k, _)| k.cmp(&name))
        .ok()
        .map(|i| entries[i].1)
}
