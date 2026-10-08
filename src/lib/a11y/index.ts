/**
 * RAVENBOT Accessibility Utilities
 * 
 * Provides keyboard navigation, screen reader support,
 * and ARIA helpers for the UI.
 */

/**
 * Trap focus within a container element
 */
export function trapFocus(container: HTMLElement): () => void {
  const focusableElements = container.querySelectorAll<HTMLElement>(
    'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
  );
  
  const firstElement = focusableElements[0];
  const lastElement = focusableElements[focusableElements.length - 1];

  function handleTab(e: KeyboardEvent) {
    if (e.key !== 'Tab') return;

    if (e.shiftKey) {
      if (document.activeElement === firstElement) {
        lastElement?.focus();
        e.preventDefault();
      }
    } else {
      if (document.activeElement === lastElement) {
        firstElement?.focus();
        e.preventDefault();
      }
    }
  }

  container.addEventListener('keydown', handleTab);
  firstElement?.focus();

  return () => {
    container.removeEventListener('keydown', handleTab);
  };
}

/**
 * Handle escape key press
 */
export function onEscape(callback: () => void): () => void {
  function handler(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      callback();
    }
  }

  document.addEventListener('keydown', handler);
  return () => document.removeEventListener('keydown', handler);
}

/**
 * Announce a message to screen readers
 */
export function announce(message: string, priority: 'polite' | 'assertive' = 'polite'): void {
  const announcement = document.createElement('div');
  announcement.setAttribute('role', 'status');
  announcement.setAttribute('aria-live', priority);
  announcement.setAttribute('aria-atomic', 'true');
  announcement.className = 'sr-only';
  announcement.textContent = message;
  
  document.body.appendChild(announcement);
  
  setTimeout(() => {
    document.body.removeChild(announcement);
  }, 1000);
}

/**
 * Generate unique ID for ARIA relationships
 */
let idCounter = 0;
export function uniqueId(prefix: string = 'rb'): string {
  return `${prefix}-${++idCounter}`;
}

/**
 * Check if reduced motion is preferred
 */
export function prefersReducedMotion(): boolean {
  if (typeof window === 'undefined') return false;
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/**
 * Check if high contrast is preferred
 */
export function prefersHighContrast(): boolean {
  if (typeof window === 'undefined') return false;
  return window.matchMedia('(prefers-contrast: high)').matches;
}

/**
 * Get contrast ratio between two colors
 */
export function getContrastRatio(color1: string, color2: string): number {
  const luminance1 = getLuminance(color1);
  const luminance2 = getLuminance(color2);
  
  const lighter = Math.max(luminance1, luminance2);
  const darker = Math.min(luminance1, luminance2);
  
  return (lighter + 0.05) / (darker + 0.05);
}

function getLuminance(color: string): number {
  // Simplified - in production use proper color parsing
  const rgb = hexToRgb(color);
  if (!rgb) return 0;
  
  const [r, g, b] = [rgb.r / 255, rgb.g / 255, rgb.b / 255].map(c =>
    c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4)
  );
  
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function hexToRgb(hex: string): { r: number; g: number; b: number } | null {
  const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  return result ? {
    r: parseInt(result[1], 16),
    g: parseInt(result[2], 16),
    b: parseInt(result[3], 16)
  } : null;
}

/**
 * Keyboard shortcut handler
 */
export class KeyboardShortcutManager {
  private shortcuts: Map<string, () => void> = new Map();

  register(combo: string, callback: () => void): () => void {
    this.shortcuts.set(combo, callback);
    
    const handler = (e: KeyboardEvent) => {
      if (this.matchesCombo(e, combo)) {
        e.preventDefault();
        callback();
      }
    };
    
    document.addEventListener('keydown', handler);
    return () => {
      document.removeEventListener('keydown', handler);
      this.shortcuts.delete(combo);
    };
  }

  private matchesCombo(e: KeyboardEvent, combo: string): boolean {
    const parts = combo.toLowerCase().split('+');
    const targetKey = parts.pop()?.toLowerCase();
    if (!targetKey) return false;

    const hasMod = parts.includes('mod');
    const hasCtrl = parts.includes('ctrl');
    const hasMeta = parts.includes('meta') || parts.includes('cmd');
    const hasShift = parts.includes('shift');
    const hasAlt = parts.includes('alt');

    const keyMatches =
      e.key.toLowerCase() === targetKey ||
      (targetKey === ',' && (e.code === 'Comma' || e.key === ',')) ||
      (targetKey === 'k' && (e.code === 'KeyK' || e.key.toLowerCase() === 'k')) ||
      (targetKey === 'b' && (e.code === 'KeyB' || e.key.toLowerCase() === 'b')) ||
      (targetKey === 'n' && (e.code === 'KeyN' || e.key.toLowerCase() === 'n')) ||
      (targetKey === 'escape' && (e.code === 'Escape' || e.key === 'Escape'));

    if (!keyMatches) return false;

    if (hasMod) {
      if (!e.ctrlKey && !e.metaKey) return false;
    } else {
      if (hasCtrl !== e.ctrlKey) return false;
      if (hasMeta !== e.metaKey) return false;
    }

    if (hasShift !== e.shiftKey) return false;
    if (hasAlt !== e.altKey) return false;

    return true;
  }
}

export const keyboardShortcuts = new KeyboardShortcutManager();
