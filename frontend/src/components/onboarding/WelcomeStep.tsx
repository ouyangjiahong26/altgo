/** 欢迎步：一句话定位与一条操作说明。
 * 品牌标记在头部已经出现，这里不再重复同一枚图标，改用大一号的标题承担开场。 */
export function WelcomeStep({ t }: { t: (key: string) => string }) {
  return (
    <div className="onboarding-step onboarding-step--welcome">
      <h2 className="onboarding-title">{t("onboarding.welcome_title")}</h2>
      <p className="onboarding-lead">{t("onboarding.welcome_text")}</p>
    </div>
  );
}
