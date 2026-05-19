# Muni

## Current Work

No current work listed.

## Owned Bugs

## Character replies lose avatar icons in conversation and roleplay

- Status: In progress
- Owner: Muni
- Impact area: UI, engine generation persistence
- Reported: 2026-05-19
- Last updated: 2026-05-19

### Steps

1. Open a conversation-mode chat with a character that has an avatar.
2. Reply to or request a specific character response.
3. Check the generated character reply in the message list.
4. Verify the same path in roleplay mode.

### Expected

Character replies should keep the character's icon/avatar image.

### Actual

Character replies could render without the character icon/avatar image after replying in conversation mode, and likely roleplay mode.

### Notes

- Initial trace points to the message `characterId` contract rather than a missing avatar asset.
- The conversation and roleplay message renderers resolve assistant avatars from `message.characterId`.
- The reply picker passes `forCharacterId` into generation, but assistant message persistence appears to save the reply without copying that target into the saved message `characterId`.
- Initial partial fix persisted requested `forCharacterId` as the assistant message `characterId` when it belongs to the chat.
- Conversation testing showed normal one-on-one replies can still miss avatars because they do not pass `forCharacterId`; fix needs to infer the single chat character when no explicit reply target exists.

## Cannot delete messages in the built-in Professor Mari chat

- Status: In progress
- Owner: Muni
- Impact area: Rust storage capability, UI message deletion
- Reported: 2026-05-19
- Last updated: 2026-05-19

### Steps

1. Open the built-in Professor Mari conversation.
2. Try to delete a message or use multi-select deletion.

### Expected

Messages in the Professor Mari conversation should be deletable while the built-in Mari character and chat remain protected.

### Actual

Deleting messages in the Professor Mari conversation fails because Rust storage rejects message deletion for the protected chat.

### Notes

- `storage_delete` blocks deleting any message whose `chatId` is the protected Professor Mari chat.
- `chat_messages_bulk_delete` blocks bulk deletion for the protected Professor Mari chat.

## Status Notes

No status notes currently listed.
