@echo off
rem 切换控制台到 UTF-8 编码页
chcp 65001 > nul
setlocal enabledelayedexpansion

rem ==========================================================================
rem  yumi_module 构建脚本
rem ==========================================================================

rem --- !!! 手动指定工具路径 !!! ---
set "ZIP_EXE_PATH=E:\Program Files\7-Zip\7z.exe"
set "STRIP_EXE_PATH=E:\work\android-ndk-r29\toolchains\llvm\prebuilt\windows-x86_64\bin\llvm-strip.exe"

@REM rem --- !!! 手动指定 NDK 编译器路径 (作为保险措施) !!! ---
@REM rem 如果 .cargo/config.toml 配置正确，这些其实不需要；
@REM rem 但为了防止再次出现 "program not found" 错误，直接在这里设置环境变量最稳妥。
@REM set "CC_PATH=E:/work/android-ndk-r29/toolchains/llvm/prebuilt/windows-x86_64/bin/aarch64-linux-android26-clang.cmd"
@REM set "CXX_PATH=E:/work/android-ndk-r29/toolchains/llvm/prebuilt/windows-x86_64/bin/aarch64-linux-android26-clang++.cmd"
@REM set "AR_PATH=E:/work/android-ndk-r29/toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-ar.exe"

@REM rem 设置环境变量供 cc crate 使用
@REM set "CC_aarch64_linux_android=%CC_PATH%"
@REM set "CXX_aarch64_linux_android=%CXX_PATH%"
@REM set "AR_aarch64_linux_android=%AR_PATH%"


rem --- 项目配置 ---
rem Rust 项目文件夹名称
set "PROJ_DIR=yumi"
rem Magisk 模块模板文件夹名称
set "MODULE_DIR=yumi_module"
set "WEBUI_DIR=yumi-webui"
rem 目标架构
set "TARGET_ARCH=aarch64-linux-android"
rem 生成的二进制文件名 (由 Cargo.toml 中的 [package] name 决定)
set "BINARY_NAME=yumi"

rem --- 路径计算 ---
set "BINARY_SRC=%PROJ_DIR%\target\%TARGET_ARCH%\release\%BINARY_NAME%"
set "TARGET_BIN_DIR=%MODULE_DIR%\core\bin"
set "BINARY_DEST=%TARGET_BIN_DIR%\%BINARY_NAME%"
set "WEBUI_DIST=%WEBUI_DIR%\dist"
set "WEBROOT_DIR=%MODULE_DIR%\webroot"


echo ========================================================
echo      开始构建 yumi_module (Unified)
echo ========================================================
echo.

echo --- 1. 编译 Rust 项目: %PROJ_DIR% ---
pushd "%PROJ_DIR%"
echo 正在执行 cargo build...
cargo build --target %TARGET_ARCH% --release
if !ERRORLEVEL! neq 0 (
    echo.
    echo [ERROR] 编译失败!
    popd
    pause
    exit /b !ERRORLEVEL!
)
popd
echo 编译成功.
echo.

echo --- 2. 准备目标目录 ---
if not exist "%TARGET_BIN_DIR%" (
    mkdir "%TARGET_BIN_DIR%"
    echo 创建目录: %TARGET_BIN_DIR%
)

echo --- 3. 复制二进制文件 ---
if not exist "%BINARY_SRC%" (
    echo [ERROR] 找不到编译好的文件: %BINARY_SRC%
    pause
    exit /b 1
)

echo 复制: %BINARY_NAME% -^> %TARGET_BIN_DIR%
copy /Y "%BINARY_SRC%" "%BINARY_DEST%" > nul
if !ERRORLEVEL! neq 0 (
    echo [ERROR] 复制文件失败.
    exit /b !ERRORLEVEL!
)
echo 复制成功.
echo.

echo --- 4. Strip 二进制文件 (减小体积) ---
if exist "%STRIP_EXE_PATH%" (
    echo 正在执行 llvm-strip...
    "%STRIP_EXE_PATH%" "%BINARY_DEST%"
) else (
    echo [警告] 未找到 strip 工具，跳过 strip 步骤.
    echo 路径: %STRIP_EXE_PATH%
)
echo.

echo --- 5. 构建并同步 WebUI ---
if exist "%WEBUI_DIR%" (
    if not exist "%WEBROOT_DIR%" mkdir "%WEBROOT_DIR%"
    pushd "%WEBUI_DIR%"
    if not exist "node_modules" (
        echo 正在执行 bun install...
        bun install
        if !ERRORLEVEL! neq 0 (
            echo [ERROR] WebUI 依赖安装失败.
            popd
            exit /b !ERRORLEVEL!
        )
    )
    echo 正在执行 bun run build...
    bun run build
    if !ERRORLEVEL! neq 0 (
        echo [ERROR] WebUI 构建失败.
        popd
        exit /b !ERRORLEVEL!
    )
    popd
    if exist "%WEBUI_DIST%" (
        if exist "%WEBROOT_DIR%" rmdir /S /Q "%WEBROOT_DIR%"
        mkdir "%WEBROOT_DIR%"
        xcopy /E /I /Y "%WEBUI_DIST%\*" "%WEBROOT_DIR%\" > nul
        if !ERRORLEVEL! neq 0 (
            echo [ERROR] WebUI 同步到 webroot 失败.
            exit /b !ERRORLEVEL!
        )
    ) else (
        echo [ERROR] WebUI dist 目录不存在: %WEBUI_DIST%
        exit /b 1
    )
) else (
    echo [警告] 未找到 WebUI 目录，跳过 WebUI 构建: %WEBUI_DIR%
)
echo.

echo --- 6. 打包 Magisk 模块 (Zip) ---
if not exist "%ZIP_EXE_PATH%" (
    echo [ERROR] 未找到 7z.exe.
    echo 路径: %ZIP_EXE_PATH%
    pause
    exit /b 1
)

rem 生成时间戳
for /f "tokens=2 delims==" %%G in ('wmic os get localdatetime /value') do set "dt=%%G"
set "TIMESTAMP=%dt:~0,4%%dt:~4,2%%dt:~6,2%-%dt:~8,2%%dt:~10,2%"
set "ZIP_FILE_NAME=yumi-%TIMESTAMP%.zip"

echo 正在打包: %ZIP_FILE_NAME% ...
if exist "%ZIP_FILE_NAME%" del /F /Q "%ZIP_FILE_NAME%"

rem 这里的 ".\%MODULE_DIR%\*" 确保压缩包根目录是模块内容，而不是 yumi 文件夹本身
"%ZIP_EXE_PATH%" a -tzip -r -mx=9 "%ZIP_FILE_NAME%" ".\%MODULE_DIR%\*" > nul

if !ERRORLEVEL! neq 0 (
    echo [ERROR] 打包失败.
    pause
    exit /b !ERRORLEVEL!
)
echo.

echo ========================================================
echo  构建完成! 
echo  输出文件: %ZIP_FILE_NAME%
echo ========================================================
echo.
pause
