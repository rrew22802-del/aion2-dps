import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import en from "./locales/en.json";
import ko from "./locales/ko.json";
import ru from "./locales/ru.json";

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      en: { ui: en },
      ko: { ui: ko },
      ru: { ui: ru },
    },
    defaultNS: "ui",
    ns: ["ui"],
    fallbackLng: "en",
    // Chinese was dropped: this build targets the global service, and every
    // remaining string is authored in English. Russian was added for
    // dbaion2.ru's own DBAion2 DPS fork.
    supportedLngs: ["en", "ko", "ru"],
    interpolation: {
      escapeValue: false,
    },
    detection: {
      order: ["localStorage", "navigator"],
      caches: ["localStorage"],
      lookupLocalStorage: "i18nextLng",
    },
  });

export default i18n;
