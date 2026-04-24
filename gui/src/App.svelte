<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";

  interface Cell {
    id: number;
    input: string;
    output: string | null;
    error: string | null;
    cellNumber: number | null;
    isEvaluating: boolean;
  }

  let nextId = 1;
  let evalCounter = $state(0);

  function makeCell(input = ""): Cell {
    return {
      id: nextId++,
      input,
      output: null,
      error: null,
      cellNumber: null,
      isEvaluating: false,
    };
  }

  let cells: Cell[] = $state([makeCell()]);
  let focusedCellId: number | null = $state(cells[0].id);
  let textareaRefs = new Map<number, HTMLTextAreaElement>();

  function autoResize(el: HTMLTextAreaElement) {
    el.style.height = "auto";
    el.style.height = Math.max(el.scrollHeight, 28) + "px";
  }

  function registerTextarea(el: HTMLTextAreaElement, cellId: number) {
    textareaRefs.set(cellId, el);
    autoResize(el);
    return {
      destroy() {
        textareaRefs.delete(cellId);
      },
    };
  }

  async function evaluateCell(cell: Cell) {
    const trimmed = cell.input.trim();
    if (!trimmed || cell.isEvaluating) return;

    evalCounter++;
    cell.cellNumber = evalCounter;
    cell.isEvaluating = true;
    cell.output = null;
    cell.error = null;

    try {
      const result: any = await invoke("eval", { input: cell.input });
      if (result.success) {
        cell.output = result.output;
      } else {
        cell.error = result.error ?? "Unknown error";
      }
    } catch (e) {
      cell.error = String(e);
    } finally {
      cell.isEvaluating = false;
    }

    const idx = cells.findIndex((c) => c.id === cell.id);
    if (idx === cells.length - 1) {
      const newCell = makeCell();
      cells.push(newCell);
      await tick();
      focusCell(newCell.id);
    } else {
      await tick();
      focusCell(cells[idx + 1].id);
    }
  }

  function focusCell(id: number) {
    focusedCellId = id;
    textareaRefs.get(id)?.focus();
  }

  async function addCellAfter(idx: number) {
    const newCell = makeCell();
    cells.splice(idx + 1, 0, newCell);
    await tick();
    focusCell(newCell.id);
  }

  function deleteCell(cell: Cell) {
    if (cells.length === 1) {
      cell.input = "";
      cell.output = null;
      cell.error = null;
      cell.cellNumber = null;
      return;
    }
    const idx = cells.findIndex((c) => c.id === cell.id);
    cells.splice(idx, 1);
    const focusIdx = Math.min(idx, cells.length - 1);
    tick().then(() => focusCell(cells[focusIdx].id));
  }

  function handleKeydown(e: KeyboardEvent, cell: Cell) {
    if (e.key === "Enter" && e.shiftKey) {
      e.preventDefault();
      evaluateCell(cell);
    }
  }
</script>

<main>
  <div class="titlebar">
    <span class="title">Syma Notebook</span>
    <span class="hint">Shift+Enter to evaluate</span>
  </div>

  <div class="notebook">
    {#each cells as cell, idx (cell.id)}
      <div
        class="cell-group"
        class:focused={focusedCellId === cell.id}
      >
        <!-- Input row -->
        <div class="cell-row input-row">
          <div class="label in-label">
            {#if cell.cellNumber !== null}
              In[{cell.cellNumber}]:=
            {:else}
              In[ ]:=
            {/if}
          </div>
          <div class="cell-body">
            <textarea
              class="cell-input"
              bind:value={cell.input}
              use:registerTextarea={cell.id}
              onkeydown={(e) => handleKeydown(e, cell)}
              oninput={(e) => autoResize(e.currentTarget)}
              onfocus={() => (focusedCellId = cell.id)}
              spellcheck={false}
              placeholder="Enter expression…"
              rows={1}
            ></textarea>
          </div>
          <div class="cell-actions">
            <button
              class="run-btn"
              title="Evaluate (Shift+Enter)"
              onclick={() => evaluateCell(cell)}
              disabled={cell.isEvaluating}
            >▶</button>
            <button
              class="del-btn"
              title="Delete cell"
              onclick={() => deleteCell(cell)}
            >✕</button>
          </div>
        </div>

        <!-- Output row -->
        {#if cell.isEvaluating}
          <div class="cell-row output-row">
            <div class="label out-label">Out[{cell.cellNumber}]=</div>
            <div class="cell-output evaluating">evaluating…</div>
          </div>
        {:else if cell.output !== null}
          <div class="cell-row output-row">
            <div class="label out-label">Out[{cell.cellNumber}]=</div>
            <div class="cell-output">{cell.output}</div>
          </div>
        {:else if cell.error !== null}
          <div class="cell-row output-row">
            <div class="label out-label err-label">Out[{cell.cellNumber}]=</div>
            <div class="cell-output error">{cell.error}</div>
          </div>
        {/if}

        <!-- Add cell below button -->
        <div class="add-row">
          <button class="add-btn" onclick={() => addCellAfter(idx)} title="Add cell below">
            +
          </button>
        </div>
      </div>
    {/each}
  </div>
</main>

<style>
  :global(body) {
    margin: 0;
    background: #1a1a1a;
    color: #d4d4d4;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    font-size: 14px;
    -webkit-font-smoothing: antialiased;
  }

  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }

  /* ── Titlebar ─────────────────────────────────────── */
  .titlebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 20px;
    background: #252526;
    border-bottom: 1px solid #3a3a3a;
    user-select: none;
    flex-shrink: 0;
  }
  .title {
    font-size: 13px;
    font-weight: 600;
    color: #cccccc;
    letter-spacing: 0.02em;
  }
  .hint {
    font-size: 11px;
    color: #555;
  }

  /* ── Notebook scroll area ─────────────────────────── */
  .notebook {
    flex: 1;
    overflow-y: auto;
    padding: 24px 0 80px;
  }

  /* ── Cell group ───────────────────────────────────── */
  .cell-group {
    position: relative;
    padding: 2px 0;
  }
  .cell-group:hover .add-row {
    opacity: 1;
  }

  /* ── Rows (label + content) ───────────────────────── */
  .cell-row {
    display: flex;
    align-items: flex-start;
    padding: 0 16px;
  }

  .label {
    width: 80px;
    flex-shrink: 0;
    font-family: "JetBrains Mono", "Fira Code", "Cascadia Code", monospace;
    font-size: 12px;
    padding-top: 5px;
    text-align: right;
    padding-right: 12px;
    user-select: none;
    white-space: nowrap;
  }
  .in-label {
    color: #569cd6;
    font-weight: 600;
  }
  .out-label {
    color: #4ec9b0;
  }
  .err-label {
    color: #f48771;
  }

  .cell-body {
    flex: 1;
    min-width: 0;
  }

  /* ── Input textarea ───────────────────────────────── */
  .input-row {
    align-items: flex-start;
    gap: 0;
  }
  .cell-input {
    width: 100%;
    padding: 4px 6px;
    background: transparent;
    color: #d4d4d4;
    border: none;
    border-left: 2px solid transparent;
    font-family: "JetBrains Mono", "Fira Code", "Cascadia Code", monospace;
    font-size: 13.5px;
    line-height: 1.6;
    resize: none;
    overflow: hidden;
    outline: none;
    transition: border-color 0.15s;
  }
  .cell-input::placeholder {
    color: #444;
  }
  .cell-input:focus {
    border-left-color: #569cd6;
    background: #1e1e2e;
  }

  /* ── Cell action buttons ──────────────────────────── */
  .cell-actions {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-top: 2px;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .cell-group:hover .cell-actions,
  .cell-group.focused .cell-actions {
    opacity: 1;
  }
  .run-btn,
  .del-btn {
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 3px;
    cursor: pointer;
    font-size: 10px;
    line-height: 1;
    transition: background 0.12s;
  }
  .run-btn {
    background: #0e639c22;
    color: #569cd6;
  }
  .run-btn:hover:not(:disabled) {
    background: #0e639c55;
  }
  .run-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .del-btn {
    background: transparent;
    color: #555;
  }
  .del-btn:hover {
    background: #f4877122;
    color: #f48771;
  }

  /* ── Output ───────────────────────────────────────── */
  .output-row {
    margin-top: 2px;
    margin-bottom: 6px;
    align-items: baseline;
  }
  .cell-output {
    flex: 1;
    font-family: "JetBrains Mono", "Fira Code", "Cascadia Code", monospace;
    font-size: 13px;
    line-height: 1.6;
    padding: 4px 6px;
    color: #d4d4d4;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .cell-output.evaluating {
    color: #555;
    font-style: italic;
  }
  .cell-output.error {
    color: #f48771;
  }

  /* ── Add-cell divider ─────────────────────────────── */
  .add-row {
    display: flex;
    align-items: center;
    padding: 4px 16px 4px 108px;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .add-btn {
    height: 16px;
    padding: 0 10px;
    background: transparent;
    color: #444;
    border: 1px dashed #333;
    border-radius: 8px;
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    transition: color 0.12s, border-color 0.12s;
  }
  .add-btn:hover {
    color: #569cd6;
    border-color: #569cd6;
  }
</style>
