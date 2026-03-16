function formatUserLabel(currentLoginUser) {
  if (!currentLoginUser?.uname) return "";
  const uid = currentLoginUser?.mid ? ` (UID: ${currentLoginUser.mid})` : "";
  return `${currentLoginUser.uname}${uid}`;
}

export function getBuyerPlaceholderText({
  cookiesLength,
  buyerLoadState,
  buyerLoadMessage,
  currentLoginUser,
}) {
  if (buyerLoadState === "auth_verified" || buyerLoadState === "buyers_loading") {
    return "正在加载实名购票人...";
  }
  if (buyerLoadState === "buyers_error") {
    return buyerLoadMessage || "实名购票人加载失败，请重新登录后重试";
  }
  if (buyerLoadState === "buyers_empty" && cookiesLength > 0) {
    const userLabel = formatUserLabel(currentLoginUser);
    return userLabel
      ? `当前登录账号：${userLabel}，该账号下暂无实名购票人，请先在 B 站会员购中添加实名购票人`
      : "该账号下暂无实名购票人，请先在 B 站会员购中添加实名购票人";
  }
  if (cookiesLength > 0) {
    return "已登录，但暂未加载到实名购票人";
  }
  return "请先完成登录后再加载实名购票人";
}

export function getLoginBannerState({
  cookiesLength,
  buyerLoadState,
  buyerLoadMessage,
  currentLoginUser,
}) {
  if (cookiesLength === 0) return null;
  if (buyerLoadState === "auth_verified" || buyerLoadState === "buyers_loading") {
    return {
      tone: "loading",
      text: "已登录成功，正在加载实名购票人与地址信息…",
    };
  }
  if (buyerLoadState === "buyers_error") {
    return {
      tone: "error",
      text: buyerLoadMessage || "实名购票人与地址加载失败，请重新登录后重试。",
    };
  }
  if (buyerLoadState === "buyers_empty") {
    const userLabel = formatUserLabel(currentLoginUser);
    return {
      tone: "warning",
      text: userLabel
        ? `已登录账号：${userLabel}，但该账号下暂无实名购票人，请先在 B 站会员购中添加实名购票人。`
        : "已登录成功，但该账号下暂无实名购票人，请先在 B 站会员购中添加实名购票人。",
    };
  }
  return {
    tone: "success",
    text: currentLoginUser?.uname
      ? `已登录账号：${formatUserLabel(currentLoginUser)}，请继续选择实名购票人与联系人信息。`
      : "已登录成功，请继续选择实名购票人与联系人信息。",
  };
}
