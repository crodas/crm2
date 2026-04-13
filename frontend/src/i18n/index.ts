import { createContext, useContext, useState, useCallback, ReactNode, createElement } from 'react'
import en from './en.json'
import es from './es.json'

type Translations = Record<string, string>

const locales: Record<string, Translations> = { en, es }

interface I18nContextValue {
  locale: string
  setLocale: (l: string) => void
  t: (key: string, params?: Record<string, string | number>) => string
}

const I18nContext = createContext<I18nContextValue>(null!)

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState(() => localStorage.getItem('locale') ?? 'es')

  const setLocale = useCallback((l: string) => {
    setLocaleState(l)
    localStorage.setItem('locale', l)
  }, [])

  const t = useCallback((key: string, params?: Record<string, string | number>): string => {
    let text = locales[locale]?.[key] ?? locales.en[key] ?? key
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        text = text.replace(`{{${k}}}`, String(v))
      }
    }
    return text
  }, [locale])

  return createElement(I18nContext.Provider, { value: { locale, setLocale, t } }, children)
}

export function useTranslation() {
  return useContext(I18nContext)
}
