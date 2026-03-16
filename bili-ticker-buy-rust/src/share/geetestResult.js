function normalizeError(error, fallbackMessage) {
  if (error instanceof Error) {
    return error;
  }
  const message =
    error?.msg || error?.message || (typeof error === "string" ? error : fallbackMessage);
  return new Error(message || fallbackMessage);
}

export function waitForGeetestResult(captchaObj, { closeGraceMs = 2000 } = {}) {
  return new Promise((resolve, reject) => {
    let settled = false;
    let closeTimer = null;

    const clearCloseTimer = () => {
      if (closeTimer) {
        clearTimeout(closeTimer);
        closeTimer = null;
      }
    };

    const resolveOnce = (value) => {
      if (settled) return;
      settled = true;
      clearCloseTimer();
      resolve(value);
    };

    const rejectOnce = (error, fallbackMessage) => {
      if (settled) return;
      settled = true;
      clearCloseTimer();
      reject(normalizeError(error, fallbackMessage));
    };

    captchaObj.onSuccess(() => {
      const validate = captchaObj.getValidate?.();
      if (!validate) {
        rejectOnce(null, "未获取到人机验证结果");
        return;
      }
      resolveOnce({
        type: "success",
        payload: {
          validate: validate.geetest_validate,
          seccode: validate.geetest_seccode,
          challenge: validate.geetest_challenge,
        },
      });
    });

    captchaObj.onError((error) => {
      rejectOnce(error, "人机验证失败");
    });

    captchaObj.onClose(() => {
      if (settled) return;
      clearCloseTimer();
      closeTimer = setTimeout(() => {
        resolveOnce({ type: "closed" });
      }, closeGraceMs);
    });
  });
}
