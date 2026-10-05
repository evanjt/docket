<script lang="ts">
  import { areaCounts } from '../lib/areas';
  import { project } from '../lib/context';
  import { href } from '../lib/route';

  const ctx = project();

  const rows = $derived(ctx.board ? areaCounts(ctx.areas, ctx.board.nodes.values()) : []);
</script>

<div class="areas">
  {#if !ctx.board}
    <p class="faint">Reading the areas</p>
  {:else if rows.length === 0}
    <div class="empty">
      <h2>No area</h2>
      <p class="muted">An area is a part of the project every item belongs to. Add one with <span class="id">docket areas add</span>.</p>
    </div>
  {:else}
    <table>
      <thead>
        <tr><th>Area</th><th>Description</th><th>Priority</th><th class="n">Open</th><th class="n">Done</th></tr>
      </thead>
      <tbody>
        {#each rows as r (r.area.id)}
          <tr>
            <td><a class="name" href={href(ctx.slug, 'work', { area: r.area.name })}>{r.area.name}</a></td>
            <td class="muted">{r.area.description ?? ''}</td>
            <td>{#if r.area.priority}<span class="pri pri-{r.area.priority}">{r.area.priority}</span>{/if}</td>
            <td class="n">{r.open}</td>
            <td class="n">{r.done}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .areas {
    padding: 22px 28px 48px;
    max-width: 1200px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th,
  td {
    padding: 8px 10px;
    text-align: left;
    border-bottom: 1px solid var(--rule);
  }

  th {
    font-size: 12.5px;
    color: var(--muted);
    border-bottom-color: var(--rule-strong);
  }

  .n {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .name {
    font-weight: 600;
  }
</style>
