import test from "node:test";
import assert from "node:assert/strict";
import { getBuyerPlaceholderText, getLoginBannerState } from "./shareLoginState.js";

test("buyer placeholder asks user to login before loading buyers", () => {
  assert.equal(
    getBuyerPlaceholderText({
      cookiesLength: 0,
      buyerLoadState: "idle",
      buyerLoadMessage: "",
      currentLoginUser: null,
    }),
    "请先完成登录后再加载实名购票人"
  );
});

test("buyer placeholder shows loading state after login", () => {
  assert.equal(
    getBuyerPlaceholderText({
      cookiesLength: 2,
      buyerLoadState: "buyers_loading",
      buyerLoadMessage: "",
      currentLoginUser: { uname: "张三", mid: "1001" },
    }),
    "正在加载实名购票人..."
  );
});

test("buyer placeholder shows empty state after successful login with no buyers", () => {
  assert.equal(
    getBuyerPlaceholderText({
      cookiesLength: 2,
      buyerLoadState: "buyers_empty",
      buyerLoadMessage: "",
      currentLoginUser: { uname: "张三", mid: "1001" },
    }),
    "当前登录账号：张三 (UID: 1001)，该账号下暂无实名购票人，请先在 B 站会员购中添加实名购票人"
  );
});

test("login banner does not show green success when buyer loading failed", () => {
  assert.deepEqual(
    getLoginBannerState({
      cookiesLength: 2,
      buyerLoadState: "buyers_error",
      buyerLoadMessage: "获取购票人失败",
      currentLoginUser: { uname: "张三", mid: "1001" },
    }),
    {
      tone: "error",
      text: "获取购票人失败",
    }
  );
});

test("login banner shows success when buyers loaded", () => {
  assert.deepEqual(
    getLoginBannerState({
      cookiesLength: 2,
      buyerLoadState: "buyers_ready",
      buyerLoadMessage: "",
      currentLoginUser: { uname: "张三", mid: "1001" },
    }),
    {
      tone: "success",
      text: "已登录账号：张三 (UID: 1001)，请继续选择实名购票人与联系人信息。",
    }
  );
});
