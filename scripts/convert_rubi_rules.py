#!/usr/bin/env python3
"""
Convert Rubi .nb (Mathematica notebook) files to Wolfram Language .m source files.

The .nb files use Mathematica's box representation (RowBox, SuperscriptBox,
FractionBox, etc.). This script:
1. Walks the Rubi directory tree for .nb files
2. Extracts Code cells containing Int[...] rules
3. Converts box representation back to Wolfram Language source text
4. Writes .m files to the output directory
5. Generates a Rust include file with rule data for the build script

Usage: python scripts/convert_rubi_rules.py <rubi_dir> <output_dir>

Example:
  python scripts/convert_rubi_rules.py /path/to/Rubi-4.16.1.0 rubi_rules/
"""

import os
import re
import sys
from pathlib import Path


# ── Box-to-WL conversion ──

def box_to_wl(box_text: str) -> str:
    """
    Convert a Wolfram Language box expression string to WL source code.

    Handles: RowBox, SuperscriptBox, FractionBox, SqrtBox, SubscriptBox,
    SubsuperscriptBox, FormBox, StyleBox, TagBox, ErrorBox, etc.
    """
    # Strip outer whitespace
    box_text = box_text.strip()

    if not box_text:
        return ""

    # Try matching known box forms in order of specificity

    # FractionBox[a, b] -> a/b
    m = re.match(r'^FractionBox\[(.*),(.*)\]$', box_text, re.DOTALL)
    if m:
        a = box_to_wl(m.group(1))
        b = box_to_wl(m.group(2))
        return f"{a}/{b}" if _is_simple(a) else f"({a})/{b}" if _is_simple(b) else f"({a})/({b})"

    # SqrtBox[a] -> Sqrt[a]
    m = re.match(r'^SqrtBox\[(.*)\]$', box_text, re.DOTALL)
    if m:
        arg = box_to_wl(m.group(1))
        return f"Sqrt[{arg}]"

    # SuperscriptBox[a, b] -> a^b
    m = re.match(r'^SuperscriptBox\[(.*),(.*)\]$', box_text, re.DOTALL)
    if m:
        base = box_to_wl(m.group(1))
        exp = box_to_wl(m.group(2))
        return f"{_parenthesize(base)}^{_parenthesize(exp)}"

    # SubscriptBox[a, b] -> a_b  (not used directly in rules but for completeness)
    m = re.match(r'^SubscriptBox\[(.*),(.*)\]$', box_text, re.DOTALL)
    if m:
        base = box_to_wl(m.group(1))
        sub = box_to_wl(m.group(2))
        return f"{base}_{{{sub}}}" if _is_complex(sub) else f"{base}_{sub}"

    # SubsuperscriptBox[a, b, c] -> a_b^c
    m = re.match(r'^SubsuperscriptBox\[(.*),(.*),(.*)\]$', box_text, re.DOTALL)
    if m:
        base = box_to_wl(m.group(1))
        sub = box_to_wl(m.group(2))
        sup = box_to_wl(m.group(3))
        return f"{base}_{{{sub}}}^{{{sup}}}"

    # UnderscriptBox[a, b] / OverscriptBox[a, b]
    m = re.match(r'^UnderscriptBox\[(.*),(.*)\]$', box_text, re.DOTALL)
    if m:
        a = box_to_wl(m.group(1))
        b = box_to_wl(m.group(2))
        return f"Underscript[{a}, {b}]"

    m = re.match(r'^OverscriptBox\[(.*),(.*)\]$', box_text, re.DOTALL)
    if m:
        a = box_to_wl(m.group(1))
        b = box_to_wl(m.group(2))
        return f"Overscript[{a}, {b}]"

    # UnderoverscriptBox[a, b, c]
    m = re.match(r'^UnderoverscriptBox\[(.*),(.*),(.*)\]$', box_text, re.DOTALL)
    if m:
        a = box_to_wl(m.group(1))
        b = box_to_wl(m.group(2))
        c = box_to_wl(m.group(3))
        return f"Underoverscript[{a}, {b}, {c}]"

    # RadicalBox[a, b] -> a ^ (1/b)  (or Sqrt[a, b])
    m = re.match(r'^RadicalBox\[(.*),(.*)\]$', box_text, re.DOTALL)
    if m:
        radicand = box_to_wl(m.group(1))
        index = box_to_wl(m.group(2))
        return f"Power[{radicand}, Rational[1, {index}]]"

    # FormBox[a, ...] -> just extract a
    m = re.match(r'^FormBox\[(.*?),(.*)\]$', box_text, re.DOTALL)
    if m:
        return box_to_wl(m.group(1))

    # StyleBox[a, ...] -> just extract a
    m = re.match(r'^StyleBox\[(.*?),(.*)\]$', box_text, re.DOTALL)
    if m:
        return box_to_wl(m.group(1))

    # TagBox[a, ...] -> just extract a
    m = re.match(r'^TagBox\[(.*?),(.*)\]$', box_text, re.DOTALL)
    if m:
        return box_to_wl(m.group(1))

    # ErrorBox[a] -> a
    m = re.match(r'^ErrorBox\[(.*)\]$', box_text, re.DOTALL)
    if m:
        return box_to_wl(m.group(1))

    # AdjustmentBox[a, ...] -> just extract a
    m = re.match(r'^AdjustmentBox\[(.*?),(.*)\]$', box_text, re.DOTALL)
    if m:
        return box_to_wl(m.group(1))

    # InterpretedBox / TemplateBox
    m = re.match(r'^InterpretedBox\[(.*)\]$', box_text, re.DOTALL)
    if m:
        return box_to_wl(m.group(1))

    # TemplateBox has complex structure, try to extract the first display arg
    m = re.match(r'^TemplateBox\[{(.*?)},.*\]$', box_text, re.DOTALL)
    if m:
        return box_to_wl(m.group(1))

    # DynamicBox / ToBoxes — skip these
    if re.match(r'^DynamicBox\[', box_text):
        return ""
    if re.match(r'^ToBoxes\[', box_text):
        return ""

    # Handle nested RowBox
    m = re.match(r'^RowBox\s*\[\s*\{(.*)\}\s*\]$', box_text, re.DOTALL)
    if m:
        return row_box_to_wl_source(box_text)

    # Handle Row[items, separator]
    m = re.match(r'^Row\s*\[\s*\{(.*?)\}\s*,\s*(.*?)\s*\]$', box_text, re.DOTALL)
    if m:
        return row_box_to_wl_source(f"RowBox[{{{m.group(1)}}}]")

    # Handle GridBox / Grid
    if re.match(r'^GridBox\[', box_text) or re.match(r'^Grid\[', box_text):
        return ""

    # Now try string matching for special operators
    # Handle "\[Ellipsis]" etc.
    box_text = re.sub(r'\\\[(\w+)\]', _special_char_replace, box_text)

    # Fraction-like boxes integrated into RowBox
    # Check for more complex patterns

    # Check if it's a string: "text"
    m = re.match(r'^"((?:[^"\\]|\\.)*)"$', box_text)
    if m:
        content = m.group(1)
        # Unescape Mathematica string escapes
        content = content.replace('\\n', '\n').replace('\\t', '\t')
        content = content.replace('\\"', '"')
        # Return non-empty strings as their content, special strings as tokens
        return content

    # Check if it's an integer
    if re.match(r'^-?\d+$', box_text):
        return box_text

    # Check if it's a real number
    if re.match(r'^-?\d+\.\d*($|[eE][+-]?\d+$)', box_text):
        return box_text

    # Check if it's a symbol (including multi-character symbols and pattern tokens)
    if re.match(r'^[a-zA-Z$][a-zA-Z0-9$]*$', box_text):
        return box_text

    # Check if it's a special named character
    if box_text.startswith('\\[') and box_text.endswith(']'):
        return _named_char_to_wl(box_text[2:-1])

    # Check if it's a special escaped form like \:xxxx
    if re.match(r'^\\:[\da-fA-F]{4}$', box_text):
        return box_text

    # Try integer with sign
    if re.match(r'^[+-]\d+$', box_text):
        return box_text

    # Fallback: treat as literal symbol or expression
    return box_text.strip()


def _is_simple(s: str) -> bool:
    """Check if a sub-expression is simple enough to not need parenthesizing."""
    return bool(re.match(r'^[a-zA-Z0-9$_.]+$', s))


def _is_complex(s: str) -> bool:
    """Check if a sub-expression needs braces when used as subscript."""
    return not bool(re.match(r'^[a-zA-Z0-9$_.]+$', s))


def _parenthesize(s: str) -> str:
    """Parenthesize if the expression is compound."""
    if _is_simple(s):
        return s
    return f"({s})"


def _special_char_replace(m):
    """Replace Mathematica special character names with WL equivalents."""
    name = m.group(1)
    mapping = {
        'Pi': 'Pi',
        'ExponentialE': 'E',
        'ImaginaryI': 'I',
        'Infinity': 'Infinity',
        'Degree': 'Degree',
        'Alpha': 'Alpha',
        'Beta': 'Beta',
        'Gamma': 'Gamma',
        'Delta': 'Delta',
        'Epsilon': 'Epsilon',
        'Zeta': 'Zeta',
        'Eta': 'Eta',
        'Theta': 'Theta',
        'Iota': 'Iota',
        'Kappa': 'Kappa',
        'Lambda': 'Lambda',
        'Mu': 'Mu',
        'Nu': 'Nu',
        'Xi': 'Xi',
        'Omicron': 'Omicron',
        'Rho': 'Rho',
        'Sigma': 'Sigma',
        'Tau': 'Tau',
        'Upsilon': 'Upsilon',
        'Phi': 'Phi',
        'Chi': 'Chi',
        'Psi': 'Psi',
        'Omega': 'Omega',
        'Alpha*': 'AlphaStar',
        'Beta*': 'BetaStar',
        'Gamma*': 'GammaStar',
        'Delta*': 'DeltaStar',
        'Epsilon*': 'EpsilonStar',
        'Zeta*': 'ZetaStar',
        'Eta*': 'EtaStar',
        'Theta*': 'ThetaStar',
        'Iota*': 'IotaStar',
        'Kappa*': 'KappaStar',
        'Lambda*': 'LambdaStar',
        'Mu*': 'MuStar',
        'Nu*': 'NuStar',
        'Xi*': 'XiStar',
        'Omicron*': 'OmicronStar',
        'Rho*': 'RhoStar',
        'Sigma*': 'SigmaStar',
        'Tau*': 'TauStar',
        'Upsilon*': 'UpsilonStar',
        'Phi*': 'PhiStar',
        'Chi*': 'ChiStar',
        'Psi*': 'PsiStar',
        'Omega*': 'OmegaStar',
        'Integral': 'Integral',
        'DifferentialD': 'DifferentialD',
        'PartialD': 'PartialD',
        'EmptySet': 'EmptySet',
        'HBar': 'HBar',
        'Arrow': 'Arrow',
        'LongRightArrow': 'LongRightArrow',
        'Rule': 'Rule',
        'NotEqual': 'NotEqual',
        'LessEqual': 'LessEqual',
        'GreaterEqual': 'GreaterEqual',
        'Times': 'Times',
        'CenterDot': 'CenterDot',
        'Square': 'Square',
        'Circle': 'Circle',
        'FilledSmallSquare': 'FilledSmallSquare',
    }
    return mapping.get(name, name)


def _named_char_to_wl(name: str) -> str:
    """Convert Mathematica named character name to WL symbol."""
    mapping = {
        'Integral': 'Integral',
        'DifferentialD': 'DifferentialD',
        'PartialD': 'PartialD',
        'Infinity': 'Infinity',
        'Pi': 'Pi',
        'ExponentialE': 'E',
        'ImaginaryI': 'I',
        'Degree': 'Degree',
        'Times': '*',
        'LongRightArrow': '->',
        'NotEqual': '!=',
        'LessEqual': '<=',
        'GreaterEqual': '>=',
        'Rule': '->',
        'FilledSmallSquare': '',
        'Ellipsis': '...',
        'Alpha': 'Alpha',
        'Beta': 'Beta',
    }
    result = mapping.get(name, name)
    return result


def convert_rowbox_to_wl(box_text: str) -> str:
    """
    Convert a cell's contents (typically a single outer expression) to WL source.

    Handles the case where the content is wrapped in a single box structure
    or is a raw string.
    """
    box_text = box_text.strip()

    # Handle raw strings (not in boxes)
    if box_text.startswith('"'):
        # It's a raw string, probably a comment or text
        return ""

    # It's a box expression — convert directly
    return box_to_wl(box_text)


# ── Notebook parsing ──

def extract_bracket_content(text: str, start: int, open_b: str = '[', close_b: str = ']') -> tuple[str, int]:
    """
    Extract content between matching brackets, handling nesting.

    Args:
        text: The text to search in
        start: Index of the opening bracket
        open_b: Opening bracket character
        close_b: Closing bracket character

    Returns:
        (content_inside_brackets, index_of_closing_bracket)
    """
    depth = 1
    i = start + 1
    while i < len(text) and depth > 0:
        if text[i] == open_b:
            depth += 1
        elif text[i] == close_b:
            depth -= 1
        if depth > 0:
            i += 1
    if depth != 0:
        raise ValueError("Unmatched brackets")
    return text[start + 1:i], i


def extract_code_cells(notebook_text: str) -> list[str]:
    """
    Extract Code cells from a Mathematica .nb file.

    Returns a list of code cell content strings (box representations).
    """
    code_cells = []
    i = 0

    while i < len(notebook_text):
        # Find Cell[BoxData[
        cell_start = notebook_text.find('Cell[BoxData[', i)
        if cell_start == -1:
            break

        # Extract the BoxData[...] content (nested brackets)
        try:
            # Find the opening bracket after 'BoxData'
            # cell_start points to 'C' of 'Cell[BoxData['
            box_open = cell_start + 12  # position of '[' after 'BoxData'
            box_content, box_end = extract_bracket_content(notebook_text, box_open)
        except (ValueError, IndexError):
            i = cell_start + 1
            continue

        # After BoxData[...], look for ", "Code" or ", "Input"
        rest = notebook_text[box_end + 1:box_end + 100]

        if ', "Code"' in rest or ', "Input"' in rest:
            code_cells.append(box_content.strip())
            i = box_end + 1
        else:
            i = cell_start + 1

    return code_cells


def unescape_wl_string(s: str) -> str:
    """Unescape special characters in a WL string."""
    # Handle common escapes
    s = s.replace('\\n', '\n')
    s = s.replace('\\t', '\t')
    s = s.replace('\\"', '"')
    s = s.replace('\\\\', '\\')
    return s


def boxexpr_to_wl_source(box_expr: str) -> str:
    """
    Convert a code cell's box expression to WL source code.

    This handles the full box structure within Cell[BoxData[...]].
    """
    box_expr = box_expr.strip()

    # The content is typically an RowBox
    if box_expr.startswith('RowBox['):
        return row_box_to_wl_source(box_expr)
    else:
        # Direct conversion
        return box_to_wl(box_expr)


def row_box_to_wl_source(rowbox_expr: str) -> str:
    """
    Convert an RowBox[...] expression to WL source.

    The children of an RowBox are separated by commas, and include
    strings, symbols, and nested boxes.
    """
    # Extract the content inside RowBox[...]
    m = re.match(r'RowBox\s*\[\s*\{(.*)\}\s*\]', rowbox_expr, re.DOTALL)
    if not m:
        return box_to_wl(rowbox_expr)

    inner = m.group(1)
    items = split_box_items(inner)
    parts = []

    for item in items:
        item = item.strip()
        if not item:
            continue

        # String literals - these are literal text in WL syntax
        if item.startswith('"'):
            s = parse_wl_string(item)
            # Filter whitespace-only strings and trivial space strings
            if s == ' ' or s == '\n' or s == '\t':
                parts.append(s)
            elif s.strip() == '':
                parts.append(s)
            else:
                parts.append(s)
        else:
            # Non-string: box or symbol — convert recursively
            wl = box_to_wl(item)
            if wl:
                parts.append(wl)

    # Join with spaces
    result = ''.join(parts)
    # Clean up: ensure spaces between tokens where needed
    return result


def split_box_items(inner: str) -> list[str]:
    """
    Split the items inside a box list, respecting nested brackets.
    Items are comma-separated.
    """
    items = []
    depth = 0
    start = 0

    i = 0
    while i < len(inner):
        c = inner[i]

        if c in ('[', '{', '('):
            depth += 1
        elif c in (']', '}', ')'):
            depth -= 1
        elif c == ',' and depth == 0:
            items.append(inner[start:i])
            start = i + 1

        i += 1

    if start < len(inner):
        items.append(inner[start:])

    return items


def parse_wl_string(s: str) -> str:
    """Parse a WL string literal, handling escapes."""
    if s.startswith('"') and s.endswith('"'):
        content = s[1:-1]
        return unescape_wl_string(content)
    return s


def extract_int_rules(source_text: str) -> list[str]:
    """
    Extract Int[...] rule definitions from WL source text.

    Returns a list of rule strings.
    """
    rules = []

    # Pattern: Int[...] := ... (possibly with /; condition)
    lines = source_text.split('\n')
    current_rule = ''
    in_rule = False
    brace_depth = 0

    for line in lines:
        stripped = line.strip()

        # Check for the start of an Int[...] := ... rule
        if 'Int[' in stripped:
            in_rule = True
            current_rule = stripped
            brace_depth = stripped.count('[') - stripped.count(']')
        elif in_rule:
            current_rule += ' ' + stripped
            brace_depth += stripped.count('[') - stripped.count(']')

            # Check if the rule is complete (:= with balanced brackets)
            # A rule ends after the condition (or result if no condition)
            if brace_depth <= 0:
                # Check if it looks like a complete rule
                if ':=' in current_rule:
                    rules.append(current_rule.strip())
                in_rule = False
                current_rule = ''
        elif stripped:
            # We might have multiple rules on one line or in other patterns
            # This is a simplification — real notebooks have one rule per cell
            pass

    return rules


def process_nb_file(filepath: str) -> str:
    """
    Process a single .nb file and extract WL source code.
    Returns the concatenated WL source for all Code cells.
    """
    with open(filepath, 'r', encoding='utf-8', errors='replace') as f:
        text = f.read()

    code_cells = extract_code_cells(text)
    wl_parts = []

    for i, cell_box_expr in enumerate(code_cells):
        try:
            wl_source = boxexpr_to_wl_source(cell_box_expr)
            if wl_source.strip() and 'Int[' in wl_source:
                wl_parts.append(wl_source.strip())
        except Exception as e:
            print(f"  Warning: Failed to convert cell {i} in {filepath}: {e}",
                  file=sys.stderr)

    return '\n\n'.join(wl_parts)


def get_category_name(rel_path: str) -> str:
    """Extract a readable category name from a relative path."""
    parts = rel_path.replace('\\', '/').split('/')
    return ' / '.join(p.strip() for p in parts)


# ── Main conversion ──

def convert_rubi_rules(rubi_dir: str, output_dir: str):
    """
    Convert all Rubi .nb files to .m files and generate Rust rule data.

    Args:
        rubi_dir: Path to the Rubi-4.16.1.0 directory
        output_dir: Path to output directory for .m files
    """
    rubi_path = Path(rubi_dir)
    output_path = Path(output_dir)
    output_path.mkdir(parents=True, exist_ok=True)

    all_rule_files = []

    for nb_file in sorted(rubi_path.rglob('*.nb')):
        rel_path = nb_file.relative_to(rubi_path)
        print(f"Processing: {rel_path}")

        try:
            wl_source = process_nb_file(str(nb_file))
        except Exception as e:
            print(f"  Error: {e}", file=sys.stderr)
            continue

        if not wl_source.strip():
            print(f"  (no Int rules found)")
            continue

        # Count rules
        rule_count = wl_source.count('Int[')
        print(f"  Found ~{rule_count} Int rules")

        # Write .m file
        # Create subdirectories matching the original structure
        m_rel_path = rel_path.with_suffix('.m')
        m_out_path = output_path / m_rel_path
        m_out_path.parent.mkdir(parents=True, exist_ok=True)

        category = get_category_name(str(rel_path.with_suffix('')))
        header = f"(* Rubi rules from: {rel_path} *)\n(* Category: {category} *)\n(* Source: Rubi-4.16.1.0 *)\n\n"

        with open(m_out_path, 'w', encoding='utf-8') as f:
            f.write(header)
            f.write(wl_source)
            f.write('\n')

        all_rule_files.append({
            'path': str(m_rel_path),
            'name': str(rel_path.with_suffix('')),
            'category': category,
            'rule_file': str(m_rel_path),
        })

    # Summary
    print(f"\n{'='*60}")
    print(f"Processed {len(all_rule_files)} files with rules")
    print(f"Output: {output_path}")

    return all_rule_files


def main():
    if len(sys.argv) < 3:
        rubi_default = "/Users/tanganke/Downloads/Rubi-4.16.1.0"
        output_default = "rubi_rules"
        print(f"Usage: {sys.argv[0]} <rubi_dir> <output_dir>")
        print(f"  Default rubi_dir: {rubi_default}")
        print(f"  Default output_dir: {output_default}")
        print()
        rubi_dir = input(f"Rubi dir [{rubi_default}]: ").strip() or rubi_default
        output_dir = input(f"Output dir [{output_default}]: ").strip() or output_default
    else:
        rubi_dir = sys.argv[1]
        output_dir = sys.argv[2]

    if not os.path.isdir(rubi_dir):
        print(f"Error: Rubi directory not found: {rubi_dir}", file=sys.stderr)
        sys.exit(1)

    convert_rubi_rules(rubi_dir, output_dir)


if __name__ == '__main__':
    main()
