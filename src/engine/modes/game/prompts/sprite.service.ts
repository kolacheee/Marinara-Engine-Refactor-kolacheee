export interface CharacterSpriteInfo {
  name: string;
  expressions: string[];
  expressionChoices: string[];
  /** Custom full-body aliases the model may intentionally choose. */
  fullBody: string[];
  /** Engine-assigned standard full-body poses; not exposed to the model. */
  automaticFullBody: string[];
}

function getSpriteExpressionGroupKey(expression: string): string | null {
  const underscoreIndex = expression.indexOf("_");
  if (underscoreIndex <= 0) return null;
  const key = expression.slice(0, underscoreIndex).trim();
  return key || null;
}

/**
 * Collapse variant filenames like joy_01 / joy_blush into the simple group key
 * that the expression agent should see. The concrete filenames stay available
 * to sprite resolution so the UI can pick a matching saved sprite at runtime.
 */
export function buildSpriteExpressionChoices(expressions: string[]): string[] {
  const groupKeys = new Map<string, { key: string; count: number }>();

  for (const expression of expressions) {
    const groupKey = getSpriteExpressionGroupKey(expression);
    if (!groupKey) continue;

    const lookupKey = groupKey.toLowerCase();
    const existing = groupKeys.get(lookupKey);
    if (existing) {
      existing.count += 1;
    } else {
      groupKeys.set(lookupKey, { key: groupKey, count: 1 });
    }
  }

  const choices: string[] = [];
  const emitted = new Set<string>();

  for (const expression of expressions) {
    const groupKey = getSpriteExpressionGroupKey(expression);
    const group = groupKey ? groupKeys.get(groupKey.toLowerCase()) : undefined;
    const choice = group && group.count > 1 ? group.key : expression;
    const choiceLookup = choice.toLowerCase();
    if (emitted.has(choiceLookup)) continue;

    choices.push(choice);
    emitted.add(choiceLookup);
  }

  return choices;
}
