export const hasMaskChar = (value) =>
  typeof value === "string" && value.includes("*");

export const pickCleanPhone = (...values) => {
  for (const value of values) {
    if (!value && value !== 0) continue;
    const str = String(value).trim();
    if (!str) continue;
    if (!hasMaskChar(str)) {
      return str;
    }
  }
  return "";
};

export const normalizeAddress = (addr) => {
  if (!addr || typeof addr !== "object") return addr;
  const cleanPhone = pickCleanPhone(
    addr.phone,
    addr.tel,
    addr.mobile,
    addr.phone_num,
    addr.contact_tel,
    addr.contact_phone
  );
  if (cleanPhone) {
    return { ...addr, phone: cleanPhone };
  }
  return { ...addr };
};

export const getAddressPhone = (addr) => {
  if (!addr) return "";
  return pickCleanPhone(
    addr.phone,
    addr.tel,
    addr.mobile,
    addr.phone_num,
    addr.contact_tel,
    addr.contact_phone
  );
};

export const getBuyerPhone = (buyer) => {
  if (!buyer) return "";
  return pickCleanPhone(
    buyer.tel,
    buyer.mobile,
    buyer.phone,
    buyer.contact_tel,
    buyer.contact_phone
  );
};

export const buildShareBuyerPayload = ({
  buyers,
  selectedBuyerIds,
  selectedAddress,
  contactName,
  contactTel,
}) => {
  const address = normalizeAddress(selectedAddress);
  const finalBuyers = buyers
    .filter((buyer) => selectedBuyerIds.includes(String(buyer.id)))
    .map((buyer) => {
      const resolvedName = contactName || buyer.name || "";
      const resolvedTel =
        contactTel || getAddressPhone(address) || getBuyerPhone(buyer) || "";
      const nextBuyer = { ...buyer };

      if (resolvedName) {
        nextBuyer.contact_name = resolvedName;
      }
      if (resolvedTel) {
        nextBuyer.tel = resolvedTel;
        nextBuyer.mobile = resolvedTel;
        nextBuyer.phone = resolvedTel;
        nextBuyer.contact_tel = resolvedTel;
      }
      if (address) {
        nextBuyer.deliver_info = {
          ...address,
          name: resolvedName || address.name,
          phone: resolvedTel || getAddressPhone(address),
          tel: resolvedTel || getAddressPhone(address),
          contact_tel: resolvedTel || getAddressPhone(address),
        };
      }
      return nextBuyer;
    });

  return {
    buyers: finalBuyers,
    deliverInfo: address
      ? {
          ...address,
          name: contactName || address.name,
          phone: contactTel || getAddressPhone(address),
          tel: contactTel || getAddressPhone(address),
          contact_tel: contactTel || getAddressPhone(address),
        }
      : null,
    contactName,
    contactTel,
  };
};
