import test from "node:test";
import assert from "node:assert/strict";
import { waitForGeetestResult } from "./geetestResult.js";

function createCaptchaMock(validateResult = null) {
  const handlers = {};
  return {
    handlers,
    onSuccess(callback) {
      handlers.success = callback;
    },
    onError(callback) {
      handlers.error = callback;
    },
    onClose(callback) {
      handlers.close = callback;
    },
    getValidate() {
      return validateResult;
    },
  };
}

test("waitForGeetestResult prefers success when close happens immediately before success", async () => {
  const captchaObj = createCaptchaMock({
    geetest_validate: "validate-token",
    geetest_seccode: "seccode-token",
    geetest_challenge: "challenge-token",
  });

  const resultPromise = waitForGeetestResult(captchaObj, { closeGraceMs: 10 });

  captchaObj.handlers.close();
  captchaObj.handlers.success();

  const result = await resultPromise;

  assert.deepEqual(result, {
    type: "success",
    payload: {
      validate: "validate-token",
      seccode: "seccode-token",
      challenge: "challenge-token",
    },
  });
});

test("waitForGeetestResult still prefers success when close arrives well before success", async () => {
  const captchaObj = createCaptchaMock({
    geetest_validate: "validate-token",
    geetest_seccode: "seccode-token",
    geetest_challenge: "challenge-token",
  });

  const resultPromise = waitForGeetestResult(captchaObj, { closeGraceMs: 1200 });

  captchaObj.handlers.close();
  setTimeout(() => {
    captchaObj.handlers.success();
  }, 1000);

  const result = await resultPromise;

  assert.deepEqual(result, {
    type: "success",
    payload: {
      validate: "validate-token",
      seccode: "seccode-token",
      challenge: "challenge-token",
    },
  });
});

test("waitForGeetestResult default grace handles delayed success after close", async () => {
  const captchaObj = createCaptchaMock({
    geetest_validate: "validate-token",
    geetest_seccode: "seccode-token",
    geetest_challenge: "challenge-token",
  });

  const resultPromise = waitForGeetestResult(captchaObj);

  captchaObj.handlers.close();
  setTimeout(() => {
    captchaObj.handlers.success();
  }, 1000);

  const result = await resultPromise;

  assert.equal(result.type, "success");
});

test("waitForGeetestResult resolves closed when user closes captcha without success", async () => {
  const captchaObj = createCaptchaMock();

  const resultPromise = waitForGeetestResult(captchaObj, { closeGraceMs: 5 });

  captchaObj.handlers.close();

  const result = await resultPromise;

  assert.deepEqual(result, { type: "closed" });
});

test("waitForGeetestResult rejects on geetest error", async () => {
  const captchaObj = createCaptchaMock();

  const resultPromise = waitForGeetestResult(captchaObj, { closeGraceMs: 5 });
  captchaObj.handlers.error({ msg: "验证服务异常" });

  await assert.rejects(resultPromise, /验证服务异常/);
});
