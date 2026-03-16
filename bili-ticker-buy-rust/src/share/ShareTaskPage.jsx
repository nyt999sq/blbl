import { useEffect, useState } from "react";
import { CheckCircle2, Clock3, Lock, QrCode, RefreshCw, ShieldAlert, Ticket, User } from "lucide-react";
import {
  fetchShareAddresses,
  fetchShareBuyers,
  fetchShareUserInfo,
  getPublicSharePreset,
  invoke,
  submitSharePreset,
} from "../platform/apiClient";
import {
  buildShareBuyerPayload,
  getAddressPhone,
  getBuyerPhone,
  normalizeAddress,
} from "./sharePayload";
import { getBuyerPlaceholderText, getLoginBannerState } from "./shareLoginState";
import { enableDocumentScroll } from "./documentScrollMode";
import SharePhoneLoginPanel from "./SharePhoneLoginPanel";
import logo from "../assets/logo.png";

const statusClassMap = {
  active: "bg-green-500/15 text-green-300 border-green-500/30",
  completed: "bg-blue-500/15 text-blue-300 border-blue-500/30",
  expired: "bg-yellow-500/15 text-yellow-300 border-yellow-500/30",
  closed: "bg-red-500/15 text-red-300 border-red-500/30",
  invalid: "bg-red-500/15 text-red-300 border-red-500/30",
};

const statusLabelMap = {
  active: "链接可用",
  completed: "已完成",
  expired: "已过期",
  closed: "已关闭",
  invalid: "链接不存在",
};

function normalizeCookies(cookieArray) {
  if (!Array.isArray(cookieArray)) return [];
  return cookieArray
    .map((item) => String(item || "").split(";")[0].trim())
    .filter(Boolean);
}

export default function ShareTaskPage({ token }) {
  const [preset, setPreset] = useState(null);
  const [pageStatus, setPageStatus] = useState("loading");
  const [pageError, setPageError] = useState("");
  const [cookies, setCookies] = useState([]);
  const [buyers, setBuyers] = useState([]);
  const [addresses, setAddresses] = useState([]);
  const [selectedBuyerIds, setSelectedBuyerIds] = useState([]);
  const [selectedAddress, setSelectedAddress] = useState(null);
  const [contactName, setContactName] = useState("");
  const [contactTel, setContactTel] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [submitResult, setSubmitResult] = useState(null);
  const [showLoginModal, setShowLoginModal] = useState(false);
  const [qrCodeUrl, setQrCodeUrl] = useState("");
  const [loginStatus, setLoginStatus] = useState("");
  const [loginMethod, setLoginMethod] = useState("qr");
  const [buyerLoadState, setBuyerLoadState] = useState("idle");
  const [buyerLoadMessage, setBuyerLoadMessage] = useState("");
  const [currentLoginUser, setCurrentLoginUser] = useState(null);

  const requiredCount = preset?.locked_task?.count || 0;

  const loadPreset = async () => {
    setPageStatus("loading");
    setPageError("");
    try {
      const result = await getPublicSharePreset(token);
      setPreset(result);
      setPageStatus("active");
    } catch (error) {
      const nextStatus = error?.statusCode === 404 ? "invalid" : error?.data?.status || "closed";
      setPageStatus(nextStatus);
      setPageError(error?.message || "链接不可用");
    }
  };

  useEffect(() => {
    loadPreset();
  }, [token]);

  useEffect(() => enableDocumentScroll(document), []);

  const selectedBuyers = buyers.filter((buyer) =>
    selectedBuyerIds.includes(String(buyer.id))
  );
  const loginBanner = getLoginBannerState({
    cookiesLength: cookies.length,
    buyerLoadState,
    buyerLoadMessage,
    currentLoginUser,
  });

  useEffect(() => {
    if (selectedAddress) {
      const phone = getAddressPhone(selectedAddress);
      if (selectedAddress.name) setContactName(selectedAddress.name);
      if (phone) setContactTel(phone);
      return;
    }

    if (selectedBuyers.length === 1) {
      setContactName(selectedBuyers[0].name || "");
      const phone = getBuyerPhone(selectedBuyers[0]);
      if (phone) setContactTel(phone);
    }
  }, [selectedAddress, selectedBuyers]);

  const startLogin = async () => {
    try {
      setShowLoginModal(true);
      setLoginStatus("正在获取二维码...");
      const [url, key] = await invoke("get_login_qrcode");
      setQrCodeUrl(url);
      setLoginStatus("请使用 B 站 App 扫码登录");
      const result = await invoke("poll_login_status", { qrcodeKey: key });
      if (result.startsWith("[") || result.startsWith("{")) {
        const cookieArray = JSON.parse(result);
        await hydrateShareLogin(cookieArray);
        setShowLoginModal(false);
      } else {
        setLoginStatus(result || "登录未完成，请重试");
      }
    } catch (error) {
      setLoginStatus(`登录失败：${error.message || error}`);
    }
  };

  const hydrateShareLogin = async (cookieArray, loginUser = null) => {
    const normalizedCookies = normalizeCookies(cookieArray);
    setCookies(normalizedCookies);
    setSelectedBuyerIds([]);
    setBuyers([]);
    setAddresses([]);
    setSelectedAddress(null);
    setCurrentLoginUser(null);
    setBuyerLoadState("auth_verified");
    setBuyerLoadMessage("");
    setLoginStatus("正在校验账号信息...");

    let resolvedLoginUser = loginUser;
    if (!resolvedLoginUser) {
      const userInfoRes = await fetchShareUserInfo(token, normalizedCookies);
      if (userInfoRes?.code !== 0 || !userInfoRes?.data) {
        setBuyerLoadState("buyers_error");
        setBuyerLoadMessage(
          userInfoRes?.message || userInfoRes?.msg || "登录已失效，请重新登录"
        );
        throw new Error(userInfoRes?.message || userInfoRes?.msg || "登录已失效，请重新登录");
      }
      resolvedLoginUser = {
        mid:
          userInfoRes.data.mid != null ? String(userInfoRes.data.mid) : "",
        uname: userInfoRes.data.uname || "未知账号",
        face: userInfoRes.data.face || null,
      };
    }

    setCurrentLoginUser(resolvedLoginUser);
    setBuyerLoadState("buyers_loading");
    setLoginStatus("已校验账号，正在拉取实名人与地址...");

    const [buyerResult, addressResult] = await Promise.allSettled([
      fetchShareBuyers(token, normalizedCookies),
      fetchShareAddresses(token, normalizedCookies),
    ]);

    const buyerRes = buyerResult.status === "fulfilled" ? buyerResult.value : null;
    const buyerList =
      buyerRes?.code === 0 && Array.isArray(buyerRes?.data?.list)
        ? buyerRes.data.list
        : buyerRes?.errno === 0 && Array.isArray(buyerRes?.data?.list)
          ? buyerRes.data.list
          : null;

    if (!buyerList) {
      const buyerError =
        buyerResult.status === "rejected"
          ? buyerResult.reason?.message || buyerResult.reason
          : buyerRes?.message || buyerRes?.msg || "获取购票人失败，请确认账号下已有实名购票人";
      setBuyerLoadState("buyers_error");
      setBuyerLoadMessage(String(buyerError));
      throw new Error(String(buyerError));
    }

    setBuyers(buyerList);

    let statusMessage =
      buyerList.length > 0
        ? "登录成功，已加载实名购票人"
        : `已登录账号：${resolvedLoginUser?.uname || "未知账号"}，但该账号下暂无实名购票人`;
    const addressRes = addressResult.status === "fulfilled" ? addressResult.value : null;
    const addressList =
      addressRes?.code === 0 && Array.isArray(addressRes?.data?.addr_list)
        ? addressRes.data.addr_list
        : addressRes?.errno === 0 && Array.isArray(addressRes?.data?.addr_list)
          ? addressRes.data.addr_list
          : null;

    if (addressList) {
      const normalized = addressList.map(normalizeAddress);
      setAddresses(normalized);
      const defaultAddress = normalized.find((item) => item.is_default);
      if (defaultAddress) {
        setSelectedAddress(defaultAddress);
      }
      statusMessage =
        buyerList.length > 0
          ? "登录成功，已加载实名购票人与地址"
          : `已登录账号：${resolvedLoginUser?.uname || "未知账号"}，但该账号下暂无实名购票人`;
    } else if (addressResult.status === "rejected") {
      statusMessage =
        buyerList.length > 0
          ? "登录成功，已加载实名购票人；地址加载失败，可稍后重试"
          : `已登录账号：${resolvedLoginUser?.uname || "未知账号"}，但该账号下暂无实名购票人`;
    }

    setBuyerLoadState(buyerList.length > 0 ? "buyers_ready" : "buyers_empty");
    setBuyerLoadMessage(statusMessage);
    setLoginStatus(statusMessage);
    return {
      statusText: statusMessage,
      buyerCount: buyerList.length,
      addressCount: Array.isArray(addressList) ? addressList.length : 0,
      userInfo: resolvedLoginUser,
    };
  };

  const toggleBuyer = (buyer) => {
    const buyerId = String(buyer.id);
    setSelectedBuyerIds((prev) => {
      if (prev.includes(buyerId)) {
        return prev.filter((item) => item !== buyerId);
      }
      if (prev.length >= requiredCount) {
        alert(`该链接已锁定 ${requiredCount} 张票，只能选择 ${requiredCount} 位购票人`);
        return prev;
      }
      return [...prev, buyerId];
    });
  };

  const handleSubmit = async () => {
    if (!preset) return;
    if (cookies.length === 0) {
      alert("请先完成登录");
      return;
    }
    if (selectedBuyerIds.length !== requiredCount) {
      alert(`请准确选择 ${requiredCount} 位购票人`);
      return;
    }
    if (!contactName.trim()) {
      alert("请填写联系人姓名");
      return;
    }
    if (!contactTel.trim()) {
      alert("请填写联系电话");
      return;
    }

    setSubmitting(true);
    try {
      const payload = buildShareBuyerPayload({
        buyers,
        selectedBuyerIds,
        selectedAddress,
        contactName: contactName.trim(),
        contactTel: contactTel.trim(),
      });

      const result = await submitSharePreset(token, {
        cookies,
        buyers: payload.buyers,
        deliver_info: payload.deliverInfo,
        contact_name: payload.contactName,
        contact_tel: payload.contactTel,
      });
      setSubmitResult(result);
      setPageStatus("completed");
    } catch (error) {
      alert(`提交失败：${error.message || error}`);
      if (error?.data?.status) {
        setPageStatus(error.data.status);
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="min-h-screen bg-gray-950 text-white px-4 py-8">
      <div className="max-w-4xl mx-auto space-y-6">
        <div className="flex items-center gap-4">
          <img src={logo} alt="logo" className="w-14 h-14 rounded-2xl shadow-lg" />
          <div>
            <h1 className="text-2xl font-bold">分享抢票配置</h1>
            <p className="text-sm text-gray-400">
              系统已预设票务信息，你只能填写自己的账号与实名购票信息。
            </p>
          </div>
        </div>

        <div
          className={`rounded-xl border px-4 py-3 text-sm inline-flex items-center gap-3 ${
            statusClassMap[pageStatus] || statusClassMap.closed
          }`}
        >
          <ShieldAlert size={16} />
          {statusLabelMap[pageStatus] || "链接状态未知"}
          {pageError && <span className="text-xs opacity-90">{pageError}</span>}
        </div>

        {preset && (
          <>
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
              <div className="rounded-2xl border border-gray-800 bg-gray-900/70 p-6 space-y-4">
                <div className="flex items-center gap-2 text-cyan-300 font-semibold">
                  <Ticket size={16} />
                  系统已预设
                </div>
                <div className="space-y-3 text-sm">
                  <div>
                    <div className="text-gray-500 text-xs mb-1">项目</div>
                    <div>{preset.locked_task.project_name}</div>
                  </div>
                  <div>
                    <div className="text-gray-500 text-xs mb-1">场次 / 票档</div>
                    <div>{preset.locked_task.screen_name} / {preset.locked_task.sku_name}</div>
                  </div>
                  <div>
                    <div className="text-gray-500 text-xs mb-1">锁定张数</div>
                    <div>{preset.locked_task.count} 张</div>
                  </div>
                  <div>
                    <div className="text-gray-500 text-xs mb-1">价格</div>
                    <div>{preset.display_snapshot.price_text}</div>
                  </div>
                  <div>
                    <div className="text-gray-500 text-xs mb-1">开抢时间</div>
                    <div>{preset.locked_task.time_start || "立即抢票"}</div>
                  </div>
                  <div className="rounded-lg border border-dashed border-gray-700 bg-black/20 p-3 text-xs text-gray-400 space-y-1">
                    {preset.display_snapshot.locked_fields_text.map((text, index) => (
                      <div key={index}>- {text}</div>
                    ))}
                  </div>
                </div>
              </div>

              <div className="rounded-2xl border border-gray-800 bg-gray-900/70 p-6 space-y-4">
                <div className="flex items-center gap-2 text-emerald-300 font-semibold">
                  <User size={16} />
                  你需要填写
                </div>
                <div className="text-sm text-gray-400 space-y-1">
                  {preset.display_snapshot.tips.map((text, index) => (
                    <div key={index}>- {text}</div>
                  ))}
                </div>
                <div className="flex rounded-xl border border-gray-800 bg-black/20 p-1">
                  <button
                    onClick={() => setLoginMethod("qr")}
                    className={`flex-1 rounded-lg px-3 py-2 text-sm font-semibold ${
                      loginMethod === "qr"
                        ? "bg-blue-600 text-white"
                        : "text-gray-300 hover:bg-gray-800"
                    }`}
                  >
                    扫码登录
                  </button>
                  <button
                    onClick={() => setLoginMethod("sms")}
                    className={`flex-1 rounded-lg px-3 py-2 text-sm font-semibold ${
                      loginMethod === "sms"
                        ? "bg-blue-600 text-white"
                        : "text-gray-300 hover:bg-gray-800"
                    }`}
                  >
                    手机号验证码登录
                  </button>
                </div>

                {loginMethod === "qr" ? (
                  <button
                    onClick={startLogin}
                    disabled={pageStatus !== "active"}
                    className={`w-full rounded-xl px-4 py-3 font-semibold flex items-center justify-center gap-2 ${
                      pageStatus !== "active"
                        ? "bg-gray-700 text-gray-400 cursor-not-allowed"
                        : "bg-blue-600 hover:bg-blue-500 text-white"
                    }`}
                  >
                    <QrCode size={18} />
                    {cookies.length > 0 ? "重新扫码登录" : "扫码登录自己的 B 站账号"}
                  </button>
                ) : (
                  <SharePhoneLoginPanel onLoginSuccess={hydrateShareLogin} />
                )}

                {loginBanner && (
                  <div
                    className={`rounded-lg px-4 py-3 text-sm ${
                      loginBanner.tone === "success"
                        ? "border border-green-500/30 bg-green-500/10 text-green-200"
                        : loginBanner.tone === "warning"
                          ? "border border-yellow-500/30 bg-yellow-500/10 text-yellow-200"
                          : loginBanner.tone === "error"
                            ? "border border-red-500/30 bg-red-500/10 text-red-200"
                            : "border border-blue-500/30 bg-blue-500/10 text-blue-200"
                    }`}
                  >
                    {loginBanner.text}
                  </div>
                )}

                {currentLoginUser && (
                  <div className="rounded-lg border border-gray-800 bg-black/20 px-4 py-3 flex items-center gap-3">
                    {currentLoginUser.face ? (
                      <img
                        src={currentLoginUser.face}
                        alt={currentLoginUser.uname}
                        className="w-10 h-10 rounded-full border border-gray-700"
                      />
                    ) : (
                      <div className="w-10 h-10 rounded-full border border-gray-700 bg-gray-800 flex items-center justify-center text-sm text-gray-300">
                        {currentLoginUser.uname?.slice(0, 1) || "?"}
                      </div>
                    )}
                    <div className="min-w-0">
                      <div className="text-sm font-medium text-white truncate">
                        {currentLoginUser.uname || "未知账号"}
                      </div>
                      <div className="text-xs text-gray-400">
                        UID: {currentLoginUser.mid || "未知"}
                      </div>
                    </div>
                  </div>
                )}
              </div>
            </div>

            <div className="rounded-2xl border border-gray-800 bg-gray-900/70 p-6 space-y-5">
              <div className="flex items-center justify-between">
                <div className="text-lg font-semibold">实名购票人与联系人信息</div>
                <div className="text-sm text-gray-400">
                  已选择 {selectedBuyerIds.length} / {requiredCount}
                </div>
              </div>

              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                <div className="space-y-3">
                  <div className="text-sm text-gray-400">请选择 {requiredCount} 位实名购票人</div>
                  <div className="rounded-xl border border-gray-800 bg-black/20 p-3 max-h-80 overflow-y-auto space-y-2">
                    {buyers.length === 0 && (
                      <div className="text-sm text-gray-500 text-center py-8">
                        {getBuyerPlaceholderText({
                          cookiesLength: cookies.length,
                          buyerLoadState,
                          buyerLoadMessage,
                          currentLoginUser,
                        })}
                      </div>
                    )}
                    {buyers.map((buyer) => {
                      const selected = selectedBuyerIds.includes(String(buyer.id));
                      return (
                        <button
                          key={buyer.id}
                          onClick={() => toggleBuyer(buyer)}
                          className={`w-full rounded-lg border px-3 py-3 text-left transition ${
                            selected
                              ? "border-cyan-500 bg-cyan-500/10"
                              : "border-gray-800 bg-gray-900 hover:border-gray-700"
                          }`}
                        >
                          <div className="font-medium">{buyer.name}</div>
                          <div className="text-xs text-gray-500 mt-1">{buyer.personal_id}</div>
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div className="space-y-4">
                  <div>
                    <div className="text-sm text-gray-400 mb-2">收货地址（如需要）</div>
                    <select
                      value={selectedAddress ? JSON.stringify(selectedAddress) : ""}
                      onChange={(e) => {
                        if (!e.target.value) {
                          setSelectedAddress(null);
                          return;
                        }
                        setSelectedAddress(normalizeAddress(JSON.parse(e.target.value)));
                      }}
                      className="w-full rounded-lg border border-gray-700 bg-gray-800 px-3 py-3 text-sm text-white focus:outline-none focus:border-cyan-500"
                    >
                      <option value="">无需地址 / 稍后再选</option>
                      {addresses.map((address) => (
                        <option key={address.id} value={JSON.stringify(address)}>
                          {address.name} - {address.phone} - {address.prov}{address.city}
                        </option>
                      ))}
                    </select>
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <div>
                      <div className="text-sm text-gray-400 mb-2">联系人姓名</div>
                      <input
                        type="text"
                        value={contactName}
                        onChange={(e) => setContactName(e.target.value)}
                        className="w-full rounded-lg border border-gray-700 bg-gray-800 px-3 py-3 text-sm text-white focus:outline-none focus:border-cyan-500"
                        placeholder="请填写联系人姓名"
                      />
                    </div>
                    <div>
                      <div className="text-sm text-gray-400 mb-2">联系电话</div>
                      <input
                        type="text"
                        value={contactTel}
                        onChange={(e) => setContactTel(e.target.value)}
                        className="w-full rounded-lg border border-gray-700 bg-gray-800 px-3 py-3 text-sm text-white focus:outline-none focus:border-cyan-500"
                        placeholder="请填写联系电话"
                      />
                    </div>
                  </div>

                  <button
                    onClick={handleSubmit}
                    disabled={pageStatus !== "active" || submitting}
                    className={`w-full rounded-xl px-4 py-3 font-semibold ${
                      pageStatus !== "active" || submitting
                        ? "bg-gray-700 text-gray-400 cursor-not-allowed"
                        : "bg-emerald-600 hover:bg-emerald-500 text-white"
                    }`}
                  >
                    {submitting ? "正在提交..." : "提交并授权抢票"}
                  </button>

                  {submitResult && (
                    <div className="rounded-xl border border-emerald-500/30 bg-emerald-500/10 p-4 space-y-2">
                      <div className="flex items-center gap-2 text-emerald-200 font-semibold">
                        <CheckCircle2 size={18} />
                        {submitResult.message}
                      </div>
                      <div className="text-sm text-emerald-100">
                        任务 ID：{submitResult.task_id}
                      </div>
                      <div className="text-sm text-emerald-100">
                        当前状态：{submitResult.task_status === "scheduled" ? "等待开抢" : "已启动"}
                      </div>
                    </div>
                  )}
                </div>
              </div>
            </div>
          </>
        )}

        {pageStatus === "loading" && (
          <div className="rounded-xl border border-gray-800 bg-gray-900/70 p-8 text-center text-gray-400 flex items-center justify-center gap-3">
            <RefreshCw size={18} className="animate-spin" />
            正在加载分享配置...
          </div>
        )}
      </div>

      {showLoginModal && (
        <div className="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-50 px-4">
          <div className="w-full max-w-md rounded-2xl border border-gray-700 bg-gray-900 p-6 text-center">
            <div className="text-xl font-bold mb-4">扫码登录 B 站账号</div>
            <div className="bg-white rounded-xl inline-flex items-center justify-center p-4 min-h-[220px] min-w-[220px]">
              {qrCodeUrl ? (
                <img
                  src={`https://api.qrserver.com/v1/create-qr-code/?size=220x220&data=${encodeURIComponent(
                    qrCodeUrl
                  )}`}
                  alt="登录二维码"
                />
              ) : (
                <Clock3 className="text-gray-400 animate-pulse" size={28} />
              )}
            </div>
            <div className="mt-4 text-sm text-gray-300">{loginStatus}</div>
            <button
              onClick={() => setShowLoginModal(false)}
              className="mt-6 px-4 py-2 rounded-lg bg-gray-800 hover:bg-gray-700 text-sm text-white"
            >
              关闭
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
