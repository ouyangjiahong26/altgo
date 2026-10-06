/** 欢迎步：应用简介。 */
export function WelcomeStep({ t }: { t: (key: string) => string }) {
  return (
    <div className="onboarding-step">
      <img src="/altgo-logo.svg" alt="" width={52} height={52} className="onboarding-logo" />
      <h2 className="onboarding-title">{t("onboarding.welcome_title")}</h2>
      <p className="onboarding-text">{t("onboarding.welcome_text")}</p>
    </div>
  );
}
