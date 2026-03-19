export function buildManualTicketInfo({
  projectId,
  projectName,
  selectedScreen,
  selectedSku,
  sanitizedBuyers,
  cookies,
  topDeliverInfo,
  topName,
  topTel,
}) {
  return {
    project_id: String(projectId),
    project_name: projectName,
    screen_id: String(selectedScreen.id),
    screen_name: selectedScreen.name,
    sku_id: String(selectedSku.id),
    sku_name: selectedSku.desc,
    count: sanitizedBuyers.length,
    buyer_info: sanitizedBuyers,
    deliver_info: topDeliverInfo,
    cookies: typeof cookies === "string" ? JSON.parse(cookies) : cookies,
    is_hot_project: Boolean(
      selectedScreen?.screen_type === 2 || selectedSku?.is_hot_project
    ),
    pay_money: selectedSku.price,
    contact_name: topName,
    contact_tel: topTel,
  };
}
