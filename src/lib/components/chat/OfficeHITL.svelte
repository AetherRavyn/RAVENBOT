<script lang="ts">
  /**
   * Human-in-the-loop cards raised by office nodes.
   *
   * An office node's tool call parks server-side until it is decided or
   * answered, so the room surfaces the request here. The owning view keeps the
   * decision state and handlers; this component only renders and forwards,
   * with the same keyed order the room has always used.
   */
  import ApprovalCard from "./ApprovalCard.svelte";
  import InteractionBlockCard from "./InteractionBlock.svelte";
  import type { PendingApproval, PendingQuestion } from "$lib/chat/pending";

  interface Props {
    approvals?: PendingApproval[];
    questions?: PendingQuestion[];
    botNameFor: (botId: string) => string;
    decidingApproval?: string | null;
    answeringQuestion?: string | null;
    onDecide: (approvalId: string, allowed: boolean) => void;
    onAnswer: (questionId: string, answer: string) => void;
    onSubmitted?: (questionId: string) => void;
  }

  let {
    approvals = [],
    questions = [],
    botNameFor,
    decidingApproval = null,
    answeringQuestion = null,
    onDecide,
    onAnswer,
    onSubmitted,
  }: Props = $props();
</script>

{#each approvals as ap (ap.id)}
  <ApprovalCard
    botName={botNameFor(ap.bot_id)}
    toolName={ap.tool_name}
    toolLabel={ap.tool_label}
    args={ap.arguments}
    risk={ap.risk}
    deciding={decidingApproval === ap.id}
    onDecide={(allowed) => onDecide(ap.id, allowed)}
  />
{/each}

{#each questions as q (q.id)}
  <InteractionBlockCard
    botName={botNameFor(q.bot_id)}
    header={q.header}
    question={q.question}
    options={q.options || []}
    allowCustom={q.allow_custom !== false}
    kind={q.kind || "question"}
    payload={q.payload || null}
    questionId={q.id}
    answering={answeringQuestion === q.id}
    onAnswer={(answer) => onAnswer(q.id, answer)}
    onSubmitted={onSubmitted ? () => onSubmitted(q.id) : undefined}
  />
{/each}
