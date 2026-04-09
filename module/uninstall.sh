#!/system/bin/sh

# 恢复 OPPO/OnePlus/Realme 的 Oiface
if [ -n "$(getprop persist.sys.oiface.enable)" ]; then
  setprop persist.sys.oiface.enable 1
fi

# 恢复小米的 Joyose 服务
PACKAGE_NAME="com.xiaomi.joyose"
if pm list packages | grep -q "$PACKAGE_NAME"; then
  pm enable "$PACKAGE_NAME" >/dev/null 2>&1
fi


echo "卸载yumi调度成功完成 请重启手机"