import type { QueuedMessageV2 } from "./agentTypesV2";

export interface AgentQueuedMessagesProps {
  messages: QueuedMessageV2[];
  isStreaming: boolean;
  onWithdraw: (id: string) => void;
}

/**
 * Steerable mid-turn message queue rendered directly above the composer (`v2.2`):
 * lets users queue steering follow-ups while the agent is streaming and withdraw
 * any queued item before it is consumed.
 */
export function AgentQueuedMessages({
  messages,
  isStreaming,
  onWithdraw,
}: AgentQueuedMessagesProps) {
  if (!messages.length) return null;

  return (
    <ul className="ak-queued-list" aria-label="Queued messages">
      {messages.map((msg) => (
        <li key={msg.id} className="ak-queued-row">
          <span className="ak-queued-arrow" aria-hidden="true">↳</span>
          <span className="ak-queued-text">{msg.content}</span>
          <span className="ak-queued-state">
            {!isStreaming
              ? "Sending…"
              : msg.steerable
                ? "Read after this step"
                : "Sent after this reply"}
          </span>
          <button
            type="button"
            className="ak-queued-remove"
            aria-label="Remove from queue"
            title="Remove from queue"
            onClick={() => onWithdraw(msg.id)}
          >
            ×
          </button>
        </li>
      ))}
    </ul>
  );
}

export default AgentQueuedMessages;
