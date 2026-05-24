import { useEffect, useState } from "react";
import { useChatStore } from "../../../../../shared/stores/chat.store";

export function useChatOverlays(activeChatId: string) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [filesOpen, setFilesOpen] = useState(false);
  const [galleryOpen, setGalleryOpen] = useState(false);
  const [wizardOpen, setWizardOpen] = useState(false);
  const [spriteArrangeMode, setSpriteArrangeMode] = useState(false);

  const newChatSetupIntent = useChatStore((state) => state.newChatSetupIntent);
  const shouldOpenSettings = useChatStore((state) => state.shouldOpenSettings);
  const shouldOpenWizard = useChatStore((state) => state.shouldOpenWizard);

  useEffect(() => {
    setSpriteArrangeMode(false);
  }, [activeChatId]);

  useEffect(() => {
    if (!activeChatId) return;

    const intent = useChatStore.getState().consumeNewChatSetupIntent(activeChatId);
    if (intent) {
      if (intent.openWizard) {
        if (intent.shortcutMode) useChatStore.getState().setShouldOpenWizardInShortcutMode(true);
        setWizardOpen(true);
      } else if (intent.openSettings) {
        setSettingsOpen(true);
      }
      return;
    }

    if (shouldOpenSettings && !newChatSetupIntent) {
      if (shouldOpenWizard) setWizardOpen(true);
      else setSettingsOpen(true);
      useChatStore.getState().setShouldOpenWizard(false);
      useChatStore.getState().setShouldOpenSettings(false);
    }
  }, [newChatSetupIntent, shouldOpenSettings, shouldOpenWizard, activeChatId]);

  return {
    settingsOpen,
    filesOpen,
    galleryOpen,
    wizardOpen,
    spriteArrangeMode,
    setSettingsOpen,
    setFilesOpen,
    setGalleryOpen,
    setWizardOpen,
    setSpriteArrangeMode,
    openSettings: () => setSettingsOpen(true),
    openFiles: () => setFilesOpen(true),
    openGallery: () => setGalleryOpen(true),
    closeSettings: () => setSettingsOpen(false),
    closeFiles: () => setFilesOpen(false),
    closeGallery: () => setGalleryOpen(false),
    finishWizard: () => {
      setWizardOpen(false);
      setSettingsOpen(true);
    },
    toggleSpriteArrange: () => setSpriteArrangeMode((current) => !current),
  };
}
