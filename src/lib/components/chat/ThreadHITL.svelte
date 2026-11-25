<script lang="ts">
  /**
   * The gates parked at the tail of a conversation.
   *
   * An approval asks before the agent acts; a question asks for the input it
   * needs to continue. Both keep the owning view's decision state and call
   * back with the row id, so this component only renders and forwards.
   */
  import ApprovalCard from "./ApprovalCard.svelte";
  import InteractionBlockCard from "./InteractionBlock.svelte";
  import type { PendingApproval, PendingQuestion } from "$lib/chat/pending";

  interface Props {
    approvals?: PendingApproval[];
    questions?: PendingQuestion[];
    botName: string;
    decidingApproval?: string | null;
    answeringQuestion?: string | null;
    onDecide: (approvalId: string, allowed: boolean) => void;
    onAnswer: (questionId: string, answer: string) => void;
  }

  let {
    approvals = [],
    questions = [],
    botName,
    decidingApproval = null,
    answeringQuestion = null,
    onDecide,
    onAnswer,
  }: Props = $props();
</script>

<!-- Approval cards: what the bot wants to do + Allow/Deny -->
{#each approvals as ap (ap.id)}
  <ApprovalCard
    botName={botName}
    toolName={ap.tool_name}
    toolLabel={ap.tool_label}
    args={ap.arguments}
    risk={ap.risk}
    deciding={decidingApproval === ap.id}
    onDecide={(allowed) => onDecide(ap.id, allowed)}
  />
{/each}

<!-- Question cards: the agent needs input to continue -->
{#each questions as q (q.id)}
  <InteractionBlockCard
    botName={botName}
    header={q.header}
    question={q.question}
    options={q.options || []}
    allowCustom={q.allow_custom !== false}
    kind={q.kind || "question"}
    payload={q.payload || null}
    answering={answeringQuestion === q.id}
    onAnswer={(answer) => onAnswer(q.id, answer)}
  />
{/each}
