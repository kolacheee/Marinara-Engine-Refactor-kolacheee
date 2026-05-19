import { useEffect, useMemo, useRef, useState } from "react";
import { Check, ChevronUp, CircleUser, FileText, Link, Plus, Send, X } from "lucide-react";
import { runProfessorMariEntry, type MariMessage } from "../../../engine/mari/mari-entry";
import { useConnections } from "../../connections/hooks/use-connections";
import { usePersonas } from "../../characters/hooks/use-characters";
import { ConversationMessage } from "../../chats/components/ConversationMessage";
import type { CharacterMap, PersonaInfo } from "../../chats/components/chat-area.types";
import type { Message } from "../../../engine/contracts/types/chat";
import { filterLanguageGenerationConnections } from "../../../shared/lib/connection-filters";
import { cn, getAvatarCropStyle, parseAvatarCropJson } from "../../../shared/lib/utils";
import { useUIStore } from "../../../shared/stores/ui.store";

const MARI_AVATAR_URL = "/sprites/mari/Mari_profile.png";
const MARI_CHARACTER_ID = "__professor_mari_shell__";

type MariAttachment = {
  id: string;
  name: string;
  type: string;
  size: number;
  content: string;
};

type MariConnection = {
  id: string;
  name?: string;
  provider?: string;
};

type MariPersona = {
  id: string;
  name: string;
  avatarPath?: string | null;
  avatarCrop?: string;
  comment?: string | null;
  description?: string | null;
};

function newId(prefix: string) {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function formatDaySeparator(value: string) {
  const date = new Date(value);
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const messageDay = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const diffDays = Math.floor((today.getTime() - messageDay.getTime()) / 86400000);
  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  return date.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric", year: "numeric" });
}

function getDayKey(value: string) {
  const date = new Date(value);
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
}

function toConversationMessage(message: MariMessage): Message {
  return {
    id: message.id,
    chatId: "professor-mari",
    role: message.role,
    characterId: message.role === "assistant" ? MARI_CHARACTER_ID : null,
    content: message.content,
    activeSwipeIndex: 0,
    swipeCount: 1,
    createdAt: message.createdAt,
    extra: {
      displayText: null,
      isGenerated: message.role === "assistant",
      tokenCount: null,
      generationInfo: null,
    },
  };
}

export function ProfessorMariSurface() {
  const { data: rawConnections } = useConnections();
  const { data: rawPersonas } = usePersonas();
  const convoGradient = useUIStore((s) => s.convoGradient);
  const theme = useUIStore((s) => s.theme);
  const [messages, setMessages] = useState<MariMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [attachments, setAttachments] = useState<MariAttachment[]>([]);
  const [selectedConnectionId, setSelectedConnectionId] = useState<string | null>(null);
  const [selectedPersonaId, setSelectedPersonaId] = useState<string | null>(null);
  const [connectionMenuOpen, setConnectionMenuOpen] = useState(false);
  const [personaMenuOpen, setPersonaMenuOpen] = useState(false);
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const canSend = draft.trim().length > 0 || attachments.length > 0;
  const connections = useMemo(
    () =>
      filterLanguageGenerationConnections((rawConnections ?? []) as MariConnection[]).sort((a, b) =>
        (a.name || a.id).localeCompare(b.name || b.id),
      ),
    [rawConnections],
  );
  const personas = useMemo(
    () => ((rawPersonas ?? []) as MariPersona[]).slice().sort((a, b) => a.name.localeCompare(b.name)),
    [rawPersonas],
  );
  const selectedConnection = connections.find((connection) => connection.id === selectedConnectionId) ?? null;
  const selectedPersona = personas.find((persona) => persona.id === selectedPersonaId) ?? null;
  const gradientStyle = useMemo(() => {
    const gradient = convoGradient[theme];
    const isDefaultDark = convoGradient.dark.from === "#0a0a0e" && convoGradient.dark.to === "#1c2133";
    const isDefaultLight = convoGradient.light.from === "#f2eff7" && convoGradient.light.to === "#eae6f0";
    if ((theme === "dark" && isDefaultDark) || (theme === "light" && isDefaultLight)) {
      return { background: "var(--secondary)" };
    }
    return { background: `linear-gradient(135deg, ${gradient.from}, ${gradient.to})` };
  }, [convoGradient, theme]);
  const characterMap: CharacterMap = useMemo(
    () =>
      new Map([
        [
          MARI_CHARACTER_ID,
          {
            name: "Professor Mari",
            avatarUrl: MARI_AVATAR_URL,
            conversationStatus: "online",
          },
        ],
      ]),
    [],
  );
  const personaInfo: PersonaInfo | undefined = useMemo(() => {
    if (!selectedPersona) return undefined;
    return {
      name: selectedPersona.name,
      description: selectedPersona.description ?? undefined,
      avatarUrl: selectedPersona.avatarPath ?? undefined,
      avatarCrop: parseAvatarCropJson(selectedPersona.avatarCrop),
    };
  }, [selectedPersona]);
  const conversationMessages = useMemo(() => messages.map(toConversationMessage), [messages]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages.length]);

  useEffect(() => {
    const input = inputRef.current;
    if (!input) return;
    input.style.height = "0px";
    input.style.height = `${Math.min(input.scrollHeight, 160)}px`;
  }, [draft]);

  const readFiles = async (files: FileList | null) => {
    if (!files?.length) return;
    const nextAttachments = await Promise.all(
      Array.from(files).map(
        (file) =>
          new Promise<MariAttachment>((resolve, reject) => {
            const finish = (content: string) =>
              resolve({
                id: newId("mari-file"),
                name: file.name,
                type: file.type || "application/octet-stream",
                size: file.size,
                content,
              });
            if (file.type.startsWith("image/")) {
              const reader = new FileReader();
              reader.onload = () => finish(String(reader.result ?? ""));
              reader.onerror = () => reject(reader.error);
              reader.readAsDataURL(file);
              return;
            }
            file.text().then(finish).catch(reject);
          }),
      ),
    );
    setAttachments((current) => [...current, ...nextAttachments]);
    if (fileInputRef.current) fileInputRef.current.value = "";
  };

  const send = () => {
    const userMessage = draft.trim() || (attachments.length > 0 ? "[attachments]" : "");
    if (!userMessage) return;
    const createdAt = new Date().toISOString();
    const context = {
      surface: "professor-mari",
      source: "titlebar",
      connection: selectedConnection
        ? {
            id: selectedConnection.id,
            name: selectedConnection.name ?? selectedConnection.id,
            provider: selectedConnection.provider ?? null,
          }
        : null,
      persona: selectedPersona
        ? {
            id: selectedPersona.id,
            name: selectedPersona.name,
            comment: selectedPersona.comment ?? null,
            description: selectedPersona.description ?? null,
          }
        : null,
      attachments: attachments.map((attachment) => ({
        name: attachment.name,
        type: attachment.type,
        size: attachment.size,
        content: attachment.content,
      })),
    };
    const user: MariMessage = {
      id: newId("mari-user"),
      role: "user",
      content: userMessage,
      createdAt,
    };
    const response = runProfessorMariEntry({
      userMessage,
      messages,
      context,
    });
    const assistant: MariMessage = {
      id: newId("mari-assistant"),
      role: "assistant",
      content: response.content,
      createdAt: response.received.createdAt,
    };
    setMessages((current) => [...current, user, assistant]);
    setDraft("");
    setAttachments([]);
    requestAnimationFrame(() => inputRef.current?.focus());
  };

  return (
    <section className="mari-chat-area relative flex h-full flex-col overflow-hidden" style={gradientStyle}>
      <div className="mari-messages-scroll flex-1 overflow-y-auto overflow-x-hidden">
        <div className="sticky top-0 z-20 flex items-center justify-between px-4 py-2">
          <div className="flex items-center gap-2 rounded-lg bg-[var(--card)]/80 px-2.5 py-1.5 shadow-sm backdrop-blur-sm dark:bg-black/30">
            <div className="relative shrink-0">
              <span className="relative block h-5 w-5 overflow-hidden rounded-full">
                <img src={MARI_AVATAR_URL} alt="" className="h-full w-full object-cover" draggable={false} />
              </span>
              <span className="absolute -bottom-0.5 -right-0.5 h-2 w-2 rounded-full bg-emerald-400 ring-[1.5px] ring-[var(--border)]" />
            </div>
            <span className="text-[0.75rem] font-medium text-[var(--foreground)]/90">Professor Mari</span>
          </div>
          <div />
        </div>

        <div className="px-0 pb-4 pt-2">
          {messages.length === 0 ? (
            <div className="px-4 pt-2">
              <p className="text-xs text-[var(--muted-foreground)]">
                This is the start of your conversation with{" "}
                <span className="font-medium text-[var(--foreground)]">Professor Mari</span>.
              </p>
            </div>
          ) : (
            conversationMessages.map((message, index) => {
              const previous = conversationMessages[index - 1];
              const showSeparator = !previous || getDayKey(previous.createdAt) !== getDayKey(message.createdAt);
              const isGrouped =
                !!previous &&
                previous.role === message.role &&
                previous.characterId === message.characterId &&
                getDayKey(previous.createdAt) === getDayKey(message.createdAt) &&
                new Date(message.createdAt).getTime() - new Date(previous.createdAt).getTime() <= 5 * 60 * 1000;
              return (
                <div key={message.id}>
                  {showSeparator && (
                    <div className="relative my-4 flex items-center px-4">
                      <div className="flex-1 border-t border-[var(--border)]/40" />
                      <span className="mx-4 text-[0.6875rem] font-semibold text-[var(--muted-foreground)]">
                        {formatDaySeparator(message.createdAt)}
                      </span>
                      <div className="flex-1 border-t border-[var(--border)]/40" />
                    </div>
                  )}
                  <ConversationMessage
                    message={message}
                    isStreaming={false}
                    isGrouped={isGrouped}
                    hideActions
                    characterMap={characterMap}
                    personaInfo={personaInfo}
                    chatCharacterIds={[MARI_CHARACTER_ID]}
                    messageIndex={index + 1}
                    messageOrderIndex={index}
                  />
                </div>
              );
            })
          )}
          <div ref={messagesEndRef} className="h-1" />
        </div>
      </div>

      <div className="mari-chat-input chat-input-container relative z-10 px-3 pb-3 md:px-[12%]">
        {(connectionMenuOpen || personaMenuOpen || mobileMenuOpen) && (
          <MariContextMenu
            connections={connections}
            personas={personas}
            selectedConnectionId={selectedConnectionId}
            selectedPersonaId={selectedPersonaId}
            mode={mobileMenuOpen ? "both" : connectionMenuOpen ? "connections" : "personas"}
            onSelectConnection={(id) => {
              setSelectedConnectionId(id);
              setConnectionMenuOpen(false);
              setMobileMenuOpen(false);
            }}
            onSelectPersona={(id) => {
              setSelectedPersonaId(id);
              setPersonaMenuOpen(false);
              setMobileMenuOpen(false);
            }}
          />
        )}

        {attachments.length > 0 && (
          <div className="mx-auto mb-2 flex max-w-3xl flex-wrap gap-2">
            {attachments.map((attachment) => (
              <div
                key={attachment.id}
                className="group flex items-center gap-1.5 rounded-lg bg-foreground/10 px-2 py-1 text-xs text-foreground/70"
              >
                <FileText size="0.875rem" className="shrink-0 text-foreground/50" />
                <span className="max-w-[9rem] truncate">{attachment.name}</span>
                <button
                  type="button"
                  onClick={() => setAttachments((current) => current.filter((item) => item.id !== attachment.id))}
                  className="rounded-full p-0.5 opacity-60 transition-opacity hover:opacity-100"
                  title="Remove attachment"
                  aria-label={`Remove ${attachment.name}`}
                >
                  <X size="0.75rem" />
                </button>
              </div>
            ))}
          </div>
        )}

        <div
          className={cn(
            "mari-chat-input-box relative mx-auto flex max-w-3xl items-center gap-1.5 rounded-2xl border-2 px-2.5 py-2.5 transition-all duration-200 sm:gap-2 sm:px-4",
            "bg-[var(--card)]",
            canSend ? "border-blue-400/30 shadow-md shadow-blue-500/5" : "border-foreground/25",
          )}
        >
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*,.txt,.md,.markdown,.json,.jsonl,.csv,.log,.xml,.yaml,.yml"
            multiple
            className="hidden"
            onChange={(event) => void readFiles(event.target.files)}
          />
          <button
            type="button"
            onClick={() => fileInputRef.current?.click()}
            className="rounded-lg p-1.5 text-foreground/40 transition-all hover:bg-foreground/10 hover:text-foreground/70 active:scale-90"
            title="Attach files"
            aria-label="Attach files"
          >
            <Plus size="1rem" />
          </button>

          <button
            type="button"
            onClick={() => {
              setConnectionMenuOpen((open) => !open);
              setPersonaMenuOpen(false);
              setMobileMenuOpen(false);
            }}
            className={cn(
              "hidden h-8 w-8 items-center justify-center rounded-xl transition-all sm:flex",
              selectedConnection
                ? "bg-foreground/10 text-foreground"
                : "text-foreground/70 hover:bg-foreground/10 hover:text-foreground",
            )}
            title={selectedConnection ? selectedConnection.name || selectedConnection.id : "Quick Connection Switcher"}
            aria-label="Quick Connection Switcher"
          >
            <Link size="1rem" />
          </button>

          <button
            type="button"
            onClick={() => {
              setPersonaMenuOpen((open) => !open);
              setConnectionMenuOpen(false);
              setMobileMenuOpen(false);
            }}
            className={cn(
              "relative hidden h-8 w-8 items-center justify-center overflow-hidden rounded-full border-2 bg-[var(--secondary)] text-foreground/60 transition-all hover:border-foreground/30 hover:opacity-90 sm:flex",
              selectedPersona ? "border-foreground/40" : "border-transparent",
            )}
            title={selectedPersona ? selectedPersona.name : "Quick Persona Switcher"}
            aria-label="Quick Persona Switcher"
          >
            {selectedPersona?.avatarPath ? (
              <img
                src={selectedPersona.avatarPath}
                alt=""
                className="h-full w-full rounded-full object-cover"
                style={getAvatarCropStyle(parseAvatarCropJson(selectedPersona.avatarCrop))}
                draggable={false}
              />
            ) : (
              <CircleUser size="1rem" />
            )}
          </button>

          <button
            type="button"
            onClick={() => {
              setMobileMenuOpen((open) => !open);
              setConnectionMenuOpen(false);
              setPersonaMenuOpen(false);
            }}
            className="flex h-8 w-8 items-center justify-center rounded-xl text-foreground/70 transition-all hover:bg-foreground/10 hover:text-foreground sm:hidden"
            title="Quick Switcher"
            aria-label="Quick Switcher"
          >
            <ChevronUp size="1rem" className={cn("transition-transform", mobileMenuOpen && "rotate-180")} />
          </button>

          <textarea
            ref={inputRef}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                send();
              }
            }}
            rows={1}
            spellCheck
            autoCorrect="on"
            placeholder="Message @Professor Mari"
            className="mari-chat-input-textarea max-h-[12.5rem] min-w-0 flex-1 resize-none bg-transparent py-0 text-sm leading-normal text-foreground/90 placeholder:text-foreground/30 outline-none"
          />

          <button
            type="button"
            onClick={send}
            disabled={!canSend}
            className={cn(
              "mari-chat-send-btn flex h-8 w-8 shrink-0 items-center justify-center rounded-xl transition-all duration-200",
              canSend ? "text-foreground hover:text-foreground/80 active:scale-90" : "text-foreground/20",
            )}
            title="Send"
            aria-label="Send"
          >
            <Send size="0.9375rem" className={cn(canSend && "translate-x-[1px]")} />
          </button>
        </div>
      </div>
    </section>
  );
}

function MariContextMenu({
  connections,
  personas,
  selectedConnectionId,
  selectedPersonaId,
  mode,
  onSelectConnection,
  onSelectPersona,
}: {
  connections: MariConnection[];
  personas: MariPersona[];
  selectedConnectionId: string | null;
  selectedPersonaId: string | null;
  mode: "connections" | "personas" | "both";
  onSelectConnection: (id: string | null) => void;
  onSelectPersona: (id: string | null) => void;
}) {
  return (
    <div className="mx-auto mb-2 grid max-h-[min(26rem,48dvh)] max-w-3xl overflow-hidden rounded-xl border border-[var(--border)] bg-[var(--card)] shadow-2xl backdrop-blur-xl sm:w-fit sm:min-w-[20rem]">
      {(mode === "connections" || mode === "both") && (
        <div className="min-w-0 border-b border-[var(--border)] last:border-b-0">
          <div className="border-b border-[var(--border)] px-3 py-2 text-[0.6875rem] font-semibold text-[var(--muted-foreground)]">
            Connections
          </div>
          <div className="max-h-56 overflow-y-auto p-1">
            <button
              type="button"
              onClick={() => onSelectConnection(null)}
              className={cn(
                "flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-xs transition-colors hover:bg-[var(--accent)]",
                selectedConnectionId === null && "font-semibold text-[var(--foreground)]",
              )}
            >
              <span className="flex-1 truncate">No connection selected</span>
              {selectedConnectionId === null && <Check size="0.75rem" />}
            </button>
            {connections.map((connection) => {
              const active = connection.id === selectedConnectionId;
              return (
                <button
                  key={connection.id}
                  type="button"
                  onClick={() => onSelectConnection(connection.id)}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-xs transition-colors hover:bg-[var(--accent)]",
                    active && "font-semibold text-[var(--foreground)]",
                  )}
                >
                  <span className="min-w-0 flex-1 truncate">{connection.name || connection.id}</span>
                  {connection.provider && (
                    <span className="shrink-0 text-[0.625rem] text-[var(--muted-foreground)]">{connection.provider}</span>
                  )}
                  {active && <Check size="0.75rem" />}
                </button>
              );
            })}
            {connections.length === 0 && (
              <div className="px-3 py-4 text-center text-[0.6875rem] italic text-[var(--muted-foreground)]">
                No connections found.
              </div>
            )}
          </div>
        </div>
      )}

      {(mode === "personas" || mode === "both") && (
        <div className="min-w-0">
          <div className="border-b border-[var(--border)] px-3 py-2 text-[0.6875rem] font-semibold text-[var(--muted-foreground)]">
            Personas
          </div>
          <div className="max-h-56 overflow-y-auto p-1">
            <button
              type="button"
              onClick={() => onSelectPersona(null)}
              className={cn(
                "flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left transition-colors hover:bg-[var(--accent)]",
                selectedPersonaId === null && "text-[var(--foreground)]",
              )}
            >
              <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full border border-[var(--border)] bg-[var(--secondary)] text-xs font-semibold text-[var(--muted-foreground)]">
                ?
              </div>
              <div className="min-w-0 flex-1">
                <div className="truncate text-xs font-semibold">No persona selected</div>
              </div>
              {selectedPersonaId === null && <Check size="0.75rem" />}
            </button>
            {personas.map((persona) => {
              const active = persona.id === selectedPersonaId;
              return (
                <button
                  key={persona.id}
                  type="button"
                  onClick={() => onSelectPersona(persona.id)}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left transition-colors hover:bg-[var(--accent)]",
                    active && "text-[var(--foreground)]",
                  )}
                >
                  {persona.avatarPath ? (
                    <div className="h-8 w-8 shrink-0 overflow-hidden rounded-full border border-[var(--border)]">
                      <img
                        src={persona.avatarPath}
                        alt=""
                        className="h-full w-full object-cover"
                        style={getAvatarCropStyle(parseAvatarCropJson(persona.avatarCrop))}
                        draggable={false}
                      />
                    </div>
                  ) : (
                    <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full border border-[var(--border)] bg-[var(--secondary)] text-xs font-semibold text-[var(--muted-foreground)]">
                      {(persona.name || "?")[0].toUpperCase()}
                    </div>
                  )}
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-xs font-semibold">{persona.name || persona.id}</div>
                    {persona.comment && (
                      <div className="truncate text-[0.625rem] text-(--muted-foreground)">{persona.comment}</div>
                    )}
                  </div>
                  {active && <Check size="0.75rem" />}
                </button>
              );
            })}
            {personas.length === 0 && (
              <div className="px-3 py-4 text-center text-[0.6875rem] italic text-(--muted-foreground)">
                No personas found.
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
