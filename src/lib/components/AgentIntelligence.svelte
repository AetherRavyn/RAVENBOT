<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import * as Card from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import { Progress } from "$lib/components/ui/progress";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import { Brain, TrendingUp, CheckCircle2, Zap, Award } from "@lucide/svelte";

  interface Props { botId: string; botName?: string; }
  let { botId, botName = "Agent" }: Props = $props();

  let intel = $state<any>(null);
  let learnings = $state<any[]>([]);

  async function load() {
    try {
      intel = await invoke("get_agent_intelligence", { botId });
      learnings = await invoke("list_agent_learnings", { botId });
    } catch {}
  }

  $effect(() => { if (botId) load(); });

  let score = $derived(intel ? Math.round(intel.intelligence_score * 100) : 50);
  let level = $derived(score < 30 ? "Novice" : score < 60 ? "Competent" : score < 85 ? "Expert" : "Master");
</script>

<div class="space-y-4">
  {#if intel}
    <Card.Root class="bg-gradient-to-br from-purple-950/30 to-indigo-950/30 border-purple-500/20">
      <Card.Header class="pb-2">
        <Card.Title class="text-sm flex items-center gap-2"><Brain class="size-4 text-purple-400" /> {botName} — Intelligence: {score}% <Badge variant="secondary" class="ml-auto bg-purple-600 text-white">{level}</Badge></Card.Title>
        <Card.Description class="text-xs">Gets smarter daily with more data — {intel.total_memories} personal + {intel.office_memories} team memories, {intel.learnings_count} learnings</Card.Description>
      </Card.Header>
      <Card.Content class="space-y-3">
        <div class="space-y-1">
          <div class="flex justify-between text-xs"><span>Intelligence</span><span>{score}%</span></div>
          <Progress value={score} class="h-2" />
        </div>
        <div class="grid grid-cols-3 gap-2 text-center">
          <div class="p-2 rounded-xl bg-card border"><div class="text-lg font-bold">{intel.tasks_today}</div><div class="text-[10px] text-muted-foreground">Tasks today</div></div>
          <div class="p-2 rounded-xl bg-card border"><div class="text-lg font-bold text-emerald-400">{intel.success_streak}</div><div class="text-[10px] text-muted-foreground">Streak</div></div>
          <div class="p-2 rounded-xl bg-card border"><div class="text-lg font-bold">{intel.total_memories + intel.office_memories}</div><div class="text-[10px] text-muted-foreground">Memories</div></div>
        </div>
        <p class="text-xs text-muted-foreground flex items-center gap-1.5"><TrendingUp class="size-3" /> Agent learns from every success/failure — success rate drives promotion to long-term memory.</p>
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Header><Card.Title class="text-sm flex items-center gap-2"><Award class="size-4" /> Recent Learnings — how it got smarter</Card.Title></Card.Header>
      <Card.Content>
        <ScrollArea class="h-[250px] pr-2">
          <div class="space-y-2">
            {#each learnings as l}
              <div class="p-2.5 rounded-xl border bg-card flex gap-2">
                <div class="size-6 rounded-full flex items-center justify-center shrink-0 {l.learning_type === 'success' ? 'bg-emerald-500/20 text-emerald-400' : 'bg-amber-500/20 text-amber-400'}">
                  {#if l.learning_type === 'success'}<CheckCircle2 class="size-3.5" />{:else}<Zap class="size-3.5" />{/if}
                </div>
                <div class="flex-1 min-w-0">
                  <div class="text-xs font-medium truncate">{l.content}</div>
                  <div class="text-[11px] text-muted-foreground flex gap-2"><span class="capitalize">{l.learning_type}</span><span>·</span><span>{new Date(l.created_at).toLocaleDateString()}</span></div>
                </div>
              </div>
            {:else}
              <div class="py-6 text-center text-sm text-muted-foreground border-2 border-dashed rounded-xl">No learnings yet — complete a task to see intelligence grow.</div>
            {/each}
          </div>
        </ScrollArea>
      </Card.Content>
    </Card.Root>
  {:else}
    <div class="p-4 text-center text-sm text-muted-foreground">Loading intelligence...</div>
  {/if}
</div>
