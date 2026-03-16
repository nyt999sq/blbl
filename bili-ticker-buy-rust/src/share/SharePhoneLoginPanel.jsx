import { useEffect, useMemo, useRef, useState } from "react";
import {
  getSmsLoginCaptcha,
  getSmsLoginCountries,
  sendSmsLoginCode,
  verifySmsLoginCode,
} from "../platform/apiClient";
import { waitForGeetestResult } from "./geetestResult";

let geetestScriptPromise = null;

function loadGeetestScript() {
  if (typeof window === "undefined") {
    return Promise.reject(new Error("当前环境不支持人机验证"));
  }
  if (window.initGeetest) {
    return Promise.resolve(window.initGeetest);
  }
  if (geetestScriptPromise) {
    return geetestScriptPromise;
  }

  geetestScriptPromise = new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = "https://static.geetest.com/static/tools/gt.js";
    script.async = true;
    script.onload = () => {
      if (window.initGeetest) {
        resolve(window.initGeetest);
      } else {
        reject(new Error("人机验证脚本加载失败"));
      }
    };
    script.onerror = () => reject(new Error("无法加载人机验证脚本"));
    document.head.appendChild(script);
  });

  return geetestScriptPromise;
}

function isValidChinesePhone(phone, cid) {
  if (cid !== "86") return /^\d{4,15}$/.test(phone);
  return /^[1][3-9][0-9]{9}$/.test(phone);
}

function getCaptchaType(captchaInfo) {
  return captchaInfo?.captcha_type || captchaInfo?.type || "";
}

export default function SharePhoneLoginPanel({ onLoginSuccess }) {
  const requestButtonRef = useRef(null);
  const [countries, setCountries] = useState([]);
  const [countryCode, setCountryCode] = useState("86");
  const [phone, setPhone] = useState("");
  const [code, setCode] = useState("");
  const [captchaInfo, setCaptchaInfo] = useState(null);
  const [captchaImageCode, setCaptchaImageCode] = useState("");
  const [captchaKey, setCaptchaKey] = useState("");
  const [statusText, setStatusText] = useState("");
  const [cooldown, setCooldown] = useState(0);
  const [loadingCaptcha, setLoadingCaptcha] = useState(false);
  const [sendingCode, setSendingCode] = useState(false);
  const [verifyingCode, setVerifyingCode] = useState(false);

  const canRequestCode = useMemo(
    () => isValidChinesePhone(phone, countryCode) && cooldown === 0 && !sendingCode,
    [phone, countryCode, cooldown, sendingCode]
  );

  const canVerifyCode = useMemo(
    () => Boolean(captchaKey) && /^\d{6}$/.test(code) && !verifyingCode,
    [captchaKey, code, verifyingCode]
  );

  useEffect(() => {
    const loadCountries = async () => {
      try {
        const result = await getSmsLoginCountries();
        const list = Array.isArray(result?.list) ? result.list : [];
        setCountries(list);
        const fallbackCode = result?.default?.country_code || list[0]?.country_code || "86";
        setCountryCode(String(fallbackCode));
      } catch (error) {
        setCountries([{ id: 1, country_code: "86", cname: "中国大陆" }]);
        setCountryCode("86");
        setStatusText(`获取区号列表失败：${error.message || error}`);
      }
    };

    loadCountries();
  }, []);

  useEffect(() => {
    if (cooldown <= 0) return undefined;
    const timer = setTimeout(() => setCooldown((prev) => Math.max(0, prev - 1)), 1000);
    return () => clearTimeout(timer);
  }, [cooldown]);

  const requestCaptcha = async () => {
    if (!isValidChinesePhone(phone, countryCode)) {
      setStatusText("请输入合法手机号");
      return;
    }

    setLoadingCaptcha(true);
    setStatusText("正在获取人机验证...");
    setCaptchaImageCode("");
    try {
      const result = await getSmsLoginCaptcha();
      setCaptchaInfo(result);
      setLoadingCaptcha(false);
      if (getCaptchaType(result) === "geetest") {
        setStatusText("正在打开人机验证...");
        await startGeetestVerification(result);
      } else {
        setStatusText("请输入图片验证码后发送短信");
      }
    } catch (error) {
      setStatusText(`获取人机验证失败：${error.message || error}`);
      setCaptchaInfo(null);
    } finally {
      setLoadingCaptcha(false);
    }
  };

  const sendCodeWithCaptcha = async (captchaPayload, overrideCaptchaInfo = null) => {
    const activeCaptchaInfo = overrideCaptchaInfo || captchaInfo;
    setSendingCode(true);
    try {
      const result = await sendSmsLoginCode({
        source: "main_web",
        tel: phone.trim(),
        cid: countryCode,
        go_url: "",
        token: activeCaptchaInfo?.token,
        validate: captchaPayload?.validate || null,
        seccode: captchaPayload?.seccode || null,
        challenge: captchaPayload?.challenge || null,
        captcha: captchaPayload?.captcha || null,
      });
      setCaptchaKey(result.captcha_key);
      setCooldown(60);
      setStatusText("验证码已发送，5 分钟内有效");
      setCaptchaInfo(null);
      setCaptchaImageCode("");
    } catch (error) {
      setStatusText(`发送验证码失败：${error.message || error}`);
    } finally {
      setSendingCode(false);
    }
  };

  const startGeetestVerification = async (overrideCaptchaInfo = null) => {
    const activeCaptchaInfo = overrideCaptchaInfo || captchaInfo;
    if (!activeCaptchaInfo?.geetest) return;
    try {
      setStatusText("请在人机验证弹层中手工完成校验...");
      const initGeetest = await loadGeetestScript();
      const result = await new Promise((resolve, reject) => {
        initGeetest(
          {
            gt: activeCaptchaInfo.geetest.gt,
            challenge: activeCaptchaInfo.geetest.challenge,
            product: "popup",
            offline: false,
            width: "100%",
            new_captcha: true,
          },
          (captchaObj) => {
            if (typeof captchaObj.appendTo === "function" && requestButtonRef.current) {
              try {
                captchaObj.appendTo(requestButtonRef.current);
              } catch (error) {
                reject(new Error(error?.message || "人机验证挂载失败"));
                return;
              }
            }
            waitForGeetestResult(captchaObj)
              .then(resolve)
              .catch(reject);
            captchaObj.onReady(() => {
              try {
                captchaObj.verify();
              } catch (error) {
                reject(new Error(error?.message || "无法打开人机验证"));
              }
            });
          }
        );
      });
      if (result?.type === "closed") {
        setStatusText("已关闭人机验证，请重新获取验证码");
        return;
      }
      await sendCodeWithCaptcha(result.payload, activeCaptchaInfo);
    } catch (error) {
      setStatusText(`人机验证失败：${error.message || error}`);
    }
  };

  const sendImageCaptchaCode = async () => {
    if (!captchaImageCode.trim()) {
      setStatusText("请输入图片验证码");
      return;
    }
    await sendCodeWithCaptcha({ captcha: captchaImageCode.trim() });
  };

  const verifyCode = async () => {
    if (!canVerifyCode) return;
    setVerifyingCode(true);
    setStatusText("正在登录...");
    try {
      const result = await verifySmsLoginCode({
        source: "main_web",
        tel: phone.trim(),
        cid: countryCode,
        code: code.trim(),
        captcha_key: captchaKey,
        go_url: "",
      });
      setStatusText("验证码通过，正在校验账号与实名信息...");
      const summary = await onLoginSuccess(result.cookies, result.user_info || null);
      setStatusText(summary?.statusText || "登录成功，请继续选择实名购票人与联系人信息。");
    } catch (error) {
      setStatusText(`短信验证码登录失败：${error.message || error}`);
    } finally {
      setVerifyingCode(false);
    }
  };

  const captchaImageUrl =
    getCaptchaType(captchaInfo) === "img" && captchaInfo?.token
      ? `https://api.bilibili.com/x/recaptcha/img?_=${Date.now()}&token=${encodeURIComponent(
          captchaInfo.token
        )}`
      : "";

  return (
    <div className="space-y-4">
      <div className="rounded-lg border border-gray-800 bg-black/20 p-4 space-y-4">
        <div className="text-sm text-gray-400">
          手机号验证码登录仅用于获取你的 B 站登录态。若触发人机验证，请按提示手工完成。
        </div>

        <div className="grid grid-cols-3 gap-3">
          <div>
            <div className="text-xs text-gray-400 mb-2">区号</div>
            <select
              value={countryCode}
              onChange={(event) => setCountryCode(event.target.value)}
              className="w-full rounded-lg border border-gray-700 bg-gray-800 px-3 py-3 text-sm text-white focus:outline-none focus:border-cyan-500"
            >
              {countries.map((country) => (
                <option key={`${country.id}-${country.country_code}`} value={country.country_code}>
                  +{country.country_code} {country.cname}
                </option>
              ))}
            </select>
          </div>
          <div className="col-span-2">
            <div className="text-xs text-gray-400 mb-2">手机号</div>
            <input
              type="text"
              value={phone}
              onChange={(event) =>
                setPhone(event.target.value.replace(/[^\d]/g, ""))
              }
              className="w-full rounded-lg border border-gray-700 bg-gray-800 px-3 py-3 text-sm text-white focus:outline-none focus:border-cyan-500"
              placeholder="请输入手机号"
            />
          </div>
        </div>

        <div className="flex gap-3">
          <button
            ref={requestButtonRef}
            onClick={requestCaptcha}
            disabled={!canRequestCode || loadingCaptcha}
            className={`rounded-lg px-4 py-3 text-sm font-semibold ${
              !canRequestCode || loadingCaptcha
                ? "bg-gray-700 text-gray-400 cursor-not-allowed"
                : "bg-cyan-600 hover:bg-cyan-500 text-white"
            }`}
          >
            {loadingCaptcha ? "准备验证..." : cooldown > 0 ? `重新发送(${cooldown})` : "获取验证码"}
          </button>
        </div>

        {getCaptchaType(captchaInfo) === "img" && (
          <div className="rounded-lg border border-gray-800 bg-gray-900/70 p-4 space-y-3">
            <div className="text-sm text-gray-300">请输入图片验证码后发送短信</div>
            {captchaImageUrl && (
              <img
                src={captchaImageUrl}
                alt="图片验证码"
                className="h-16 rounded border border-gray-700 bg-white"
              />
            )}
            <div className="flex gap-3">
              <input
                type="text"
                value={captchaImageCode}
                onChange={(event) => setCaptchaImageCode(event.target.value)}
                className="flex-1 rounded-lg border border-gray-700 bg-gray-800 px-3 py-3 text-sm text-white focus:outline-none focus:border-cyan-500"
                placeholder="输入图片中的内容"
              />
              <button
                onClick={sendImageCaptchaCode}
                disabled={sendingCode}
                className={`rounded-lg px-4 py-3 text-sm font-semibold ${
                  sendingCode
                    ? "bg-gray-700 text-gray-400 cursor-not-allowed"
                    : "bg-blue-600 hover:bg-blue-500 text-white"
                }`}
              >
                {sendingCode ? "发送中..." : "发送验证码"}
              </button>
            </div>
          </div>
        )}

        <div>
          <div className="text-xs text-gray-400 mb-2">短信验证码</div>
          <input
            type="text"
            value={code}
            onChange={(event) => setCode(event.target.value.replace(/[^\d]/g, "").slice(0, 6))}
            className="w-full rounded-lg border border-gray-700 bg-gray-800 px-3 py-3 text-sm text-white focus:outline-none focus:border-cyan-500"
            placeholder="请输入 6 位验证码"
          />
        </div>

        <button
          onClick={verifyCode}
          disabled={!canVerifyCode}
          className={`w-full rounded-lg px-4 py-3 text-sm font-semibold ${
            !canVerifyCode
              ? "bg-gray-700 text-gray-400 cursor-not-allowed"
              : "bg-emerald-600 hover:bg-emerald-500 text-white"
          }`}
        >
          {verifyingCode ? "登录中..." : "登录并继续"}
        </button>

        {statusText && (
          <div className="rounded-lg border border-gray-800 bg-gray-900/70 px-4 py-3 text-sm text-gray-300">
            {statusText}
          </div>
        )}
      </div>
    </div>
  );
}
