#include <stdbool.h>
#include <stdatomic.h>
static atomic_bool ADUseEnglish = false;
void ad_set_locale(bool english) { atomic_store(&ADUseEnglish, english); }
#import <NaturalLanguage/NaturalLanguage.h>
#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <Vision/Vision.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import <CoreServices/CoreServices.h>

typedef void (*ADCallback)(void *, int, const unsigned char *, size_t);

// Each session owns its Rust callback context. End is emitted exactly once and
// no callback may execute after it; Rust frees that context on tag 5.
@interface ADSession : NSObject
@property void *context;
@property ADCallback callback;
@property BOOL ended;
- (void)send:(int)tag data:(NSData *)data;
- (void)text:(int)tag value:(NSString *)value;
- (void)end;
@end
@implementation ADSession
- (void)send:(int)tag data:(NSData *)data {
    @synchronized(self) {
        if (_ended) return;
        if (tag == 5) _ended = YES;
        _callback(_context, tag, data.bytes, data.length);
    }
}
- (void)text:(int)tag value:(NSString *)value {
    [self send:tag data:[value dataUsingEncoding:NSUTF8StringEncoding]];
}
- (void)end { [self send:5 data:nil]; }
@end

static ADSession *ADMakeSession(void *context, ADCallback callback) {
    ADSession *s = [ADSession new]; s.context = context; s.callback = callback; return s;
}

unsigned int ad_permissions(void) {
    return (AXIsProcessTrusted() ? 1 : 0) | (CGPreflightScreenCaptureAccess() ? 2 : 0);
}

size_t ad_dictionary_lookup(const char *input, unsigned char *output, size_t capacity) {
    @autoreleasepool {
        NSString *text = [NSString stringWithUTF8String:input];
        if (!text.length || !output || !capacity) return 0;
        CFStringRef definition = DCSCopyTextDefinition(NULL, (__bridge CFStringRef)text, CFRangeMake(0, text.length));
        if (!definition) return 0;
        NSData *data = [(__bridge NSString *)definition dataUsingEncoding:NSUTF8StringEncoding];
        size_t length = data.length;
        if (length < capacity) memcpy(output, data.bytes, length);
        CFRelease(definition);
        return MIN(length, capacity);
    }
}

void ad_open_dictionary(int kind, const char *input) {
    NSString *text = [NSString stringWithUTF8String:input];
    dispatch_async(dispatch_get_main_queue(), ^{
        if (kind == 0) {
            NSURL *url = [[NSWorkspace sharedWorkspace] URLForApplicationWithBundleIdentifier:@"com.apple.Dictionary"];
            if (url) [[NSWorkspace sharedWorkspace] openURL:url];
        } else {
            NSURLComponents *url = [NSURLComponents new];
            url.scheme = @"https"; url.host = @"www.youdao.com"; url.path = @"/result";
            url.queryItems = @[[NSURLQueryItem queryItemWithName:@"word" value:text ?: @""], [NSURLQueryItem queryItemWithName:@"lang" value:@"en"]];
            [[NSWorkspace sharedWorkspace] openURL:url.URL];
        }
    });
}

void ad_request_permission(int kind) {
    dispatch_async(dispatch_get_main_queue(), ^{
        if (kind == 1) {
            NSDictionary *options = @{ (__bridge NSString *)kAXTrustedCheckOptionPrompt: @YES };
            AXIsProcessTrustedWithOptions((__bridge CFDictionaryRef)options);
            [[NSWorkspace sharedWorkspace] openURL:[NSURL URLWithString:@"x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"]];
        } else if (kind == 2) {
            CGRequestScreenCaptureAccess();
            [[NSWorkspace sharedWorkspace] openURL:[NSURL URLWithString:@"x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"]];
        }
    });
}

void ad_copy_text(const char *text) {
    NSString *value = [NSString stringWithUTF8String:text];
    void (^write)(void) = ^{
        NSPasteboard *pb = [NSPasteboard generalPasteboard];
        [pb clearContents]; [pb setString:value forType:NSPasteboardTypeString];
    };
    if (NSThread.isMainThread) write(); else dispatch_sync(dispatch_get_main_queue(), write);
}

void ad_configure_window(void *ptr) {
    NSWindow *window = (__bridge NSWindow *)ptr;
    window.collectionBehavior |= NSWindowCollectionBehaviorMoveToActiveSpace | NSWindowCollectionBehaviorFullScreenAuxiliary;
    window.hidesOnDeactivate = NO;
}

void ad_present_window(void *ptr) {
    NSWindow *window = (__bridge NSWindow *)ptr;
    [NSApp activateIgnoringOtherApps:YES];
    [window makeKeyAndOrderFront:nil];
    [window orderFrontRegardless];
}

static BOOL ADSameApp(pid_t pid) {
    return NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier == pid;
}

static BOOL ADSelectionBusy = NO;

static void ADFinishSelection(ADSession *s, NSString *text, NSString *error) {
    if (error) [s text:3 value:error];
    else if (text.length) [s text:2 value:text];
    else [s text:3 value:@"没有读取到选中文字，请重新选择或使用截图。"];
    ADSelectionBusy = NO;
    [s end];
}

static NSArray<NSPasteboardItem *> *ADBackupPasteboard(NSPasteboard *pb, NSInteger expectedCount, NSString **error) {
    if (error) *error = nil;
    NSString *const changedError = @"剪贴板或前台应用已改变，请重试。";
    if (pb.changeCount != expectedCount) { if (error) *error = changedError; return nil; }
    NSArray<NSPasteboardItem *> *items = pb.pasteboardItems;
    if (pb.changeCount != expectedCount) { if (error) *error = changedError; return nil; }
    // nil means retrieval failed. Only an actual empty array is an empty backup.
    if (!items) { if (error) *error = @"无法读取当前剪贴板，已保留原内容。请重试或使用截图翻译。"; return nil; }
    NSMutableArray<NSPasteboardItem *> *backup = [NSMutableArray new];
    const NSUInteger limit = 8 * 1024 * 1024;
    NSUInteger bytes = 0;
    for (NSPasteboardItem *item in items) {
        NSPasteboardItem *copy = [NSPasteboardItem new];
        NSArray<NSPasteboardType> *types = item.types;
        if (pb.changeCount != expectedCount) { if (error) *error = changedError; return nil; }
        for (NSPasteboardType type in types) {
            if ([type.lowercaseString containsString:@"promise"]) { if (error) *error = @"剪贴板含延迟数据，已保留剪贴板。请使用截图翻译。"; return nil; }
            NSData *data = [item dataForType:type];
            // A pasteboard owner change makes old items stale, returning nil.
            // Report that race before classifying a representation as unreadable.
            if (pb.changeCount != expectedCount) { if (error) *error = changedError; return nil; }
            if (!data) { if (error) *error = @"剪贴板部分格式无法读取，已保留原内容。请使用截图翻译。"; return nil; }
            if (data.length > limit - bytes) { if (error) *error = @"剪贴板内容超过 8 MiB，已保留原内容。请使用截图翻译。"; return nil; }
            bytes += data.length;
            if (![copy setData:data forType:type]) { if (error) *error = @"剪贴板部分格式无法读取，已保留原内容。请使用截图翻译。"; return nil; }
        }
        [backup addObject:copy];
    }
    if (pb.changeCount != expectedCount) { if (error) *error = changedError; return nil; }
    return backup;
}

// Runs only on main. Snapshot all concrete representations with an aggregate size
// limit; reject promised/unavailable data. Restoration uses the change-count and
// input-monitoring guards below.
static void ADCopyFallback(ADSession *s, pid_t pid, CFAbsoluteTime deadline) {
    if (!ADSameApp(pid)) { ADFinishSelection(s, nil, @"取词时前台应用已改变，请重新选择。") ; return; }
    NSEventModifierFlags held = NSEvent.modifierFlags & (NSEventModifierFlagCommand | NSEventModifierFlagControl | NSEventModifierFlagOption | NSEventModifierFlagShift);
    if (held) {
        if (CFAbsoluteTimeGetCurrent() >= deadline) { ADFinishSelection(s, nil, @"请松开快捷键后重试，或使用截图。") ; return; }
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 15 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{ ADCopyFallback(s, pid, deadline); });
        return;
    }
    NSPasteboard *pb = NSPasteboard.generalPasteboard;
    NSInteger originalCount = pb.changeCount;
    NSString *backupError = nil;
    NSArray<NSPasteboardItem *> *backup = ADBackupPasteboard(pb, originalCount, &backupError);
    if (!backup) { ADFinishSelection(s, nil, backupError); return; }
    if (pb.changeCount != originalCount || !ADSameApp(pid)) { ADFinishSelection(s, nil, @"剪贴板或前台应用已改变，请重试。") ; return; }
    CGEventRef down = CGEventCreateKeyboardEvent(NULL, 8, true);
    CGEventRef up = CGEventCreateKeyboardEvent(NULL, 8, false);
    if (!down || !up) {
        if (down) CFRelease(down); if (up) CFRelease(up);
        ADFinishSelection(s, nil, @"无法发送复制快捷键，请使用截图。") ; return;
    }
    CGEventSetFlags(down, kCGEventFlagMaskCommand);
    CGEventSetFlags(up, kCGEventFlagMaskCommand);
    const int64_t copyMarker = 0x4155544f44494354;
    CGEventSetIntegerValueField(down, kCGEventSourceUserData, copyMarker);
    CGEventSetIntegerValueField(up, kCGEventSourceUserData, copyMarker);
    __block BOOL userIntervened = NO;
    id monitor = [NSEvent addGlobalMonitorForEventsMatchingMask:(NSEventMaskKeyDown | NSEventMaskLeftMouseDown | NSEventMaskRightMouseDown | NSEventMaskOtherMouseDown) handler:^(NSEvent *event) {
        CGEventRef cg = event.CGEvent;
        if (!cg || CGEventGetIntegerValueField(cg, kCGEventSourceUserData) != copyMarker) userIntervened = YES;
    }];
    if (!monitor) {
        CFRelease(down); CFRelease(up);
        ADFinishSelection(s, nil, @"无法安全监测复制操作，请使用截图翻译。") ; return;
    }
    CGEventPost(kCGHIDEventTap, down); CGEventPost(kCGHIDEventTap, up);
    CFRelease(down); CFRelease(up);

    __block void (^poll)(void);
    poll = ^{
        if (!ADSameApp(pid) || userIntervened) { [NSEvent removeMonitor:monitor]; poll = nil; ADFinishSelection(s, nil, @"检测到新的操作，已停止取词并保留当前剪贴板。") ; return; }
        NSInteger copiedCount = pb.changeCount;
        if (copiedCount != originalCount) {
            NSString *result = [pb stringForType:NSPasteboardTypeString];
            if (!userIntervened && pb.changeCount == copiedCount) {
                [pb clearContents];
                if (backup.count) [pb writeObjects:backup];
            }
            [NSEvent removeMonitor:monitor];
            poll = nil;
            ADFinishSelection(s, result, nil);
            return;
        }
        if (CFAbsoluteTimeGetCurrent() >= deadline) { [NSEvent removeMonitor:monitor]; poll = nil; ADFinishSelection(s, nil, @"该应用未提供选中文字。请使用截图翻译。") ; return; }
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 15 * NSEC_PER_MSEC), dispatch_get_main_queue(), poll);
    };
    poll();
}

int ad_selection_target_pid(void) {
    __block pid_t pid = 0;
    void (^read)(void) = ^{ pid = NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier; };
    if (NSThread.isMainThread) read(); else dispatch_sync(dispatch_get_main_queue(), read);
    return pid;
}

void ad_select_text_for_pid(int target_pid, void *context, ADCallback callback) {
    ADSession *s = ADMakeSession(context, callback);
    dispatch_async(dispatch_get_main_queue(), ^{
        if (ADSelectionBusy) { [s text:3 value:@"正在读取选中文字，请稍候。"] ; [s end]; return; }
        if (!AXIsProcessTrusted()) { [s text:3 value:@"请先在设置中开启 Yurelay 的辅助功能权限。"] ; [s end]; return; }
        pid_t pid = target_pid;
        if (pid <= 0) { [s text:3 value:@"没有可读取的前台应用，请重新选中文字。"] ; [s end]; return; }
        if (pid == getpid()) { [s text:3 value:@"请在其他应用中选中文字，再按划词快捷键。"] ; [s end]; return; }
        ADSelectionBusy = YES;
        CFAbsoluteTime deadline = CFAbsoluteTimeGetCurrent() + 0.8;
        dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
            @autoreleasepool {
                AXUIElementRef app = AXUIElementCreateApplication(pid);
                AXUIElementSetMessagingTimeout(app, 0.12);
                CFTypeRef focused = NULL;
                NSString *text = nil;
                BOOL secure = NO;
                if (AXUIElementCopyAttributeValue(app, kAXFocusedUIElementAttribute, &focused) == kAXErrorSuccess && focused && CFGetTypeID(focused) == AXUIElementGetTypeID()) {
                    AXUIElementRef element = (AXUIElementRef)focused;
                    AXUIElementSetMessagingTimeout(element, 0.12);
                    CFTypeRef subrole = NULL;
                    if (AXUIElementCopyAttributeValue(element, kAXSubroleAttribute, &subrole) == kAXErrorSuccess && subrole) {
                        secure = CFEqual(subrole, kAXSecureTextFieldSubrole); CFRelease(subrole);
                    }
                    CFTypeRef selected = NULL;
                    if (!secure && AXUIElementCopyAttributeValue(element, kAXSelectedTextAttribute, &selected) == kAXErrorSuccess && selected) {
                        if (CFGetTypeID(selected) == CFStringGetTypeID()) text = [(__bridge NSString *)selected copy];
                        CFRelease(selected);
                    }
                }
                if (focused) CFRelease(focused); CFRelease(app);
                dispatch_async(dispatch_get_main_queue(), ^{
                    if (secure) { ADFinishSelection(s, nil, @"密码输入框不支持取词。") ; return; }
                    if (!ADSameApp(pid)) { ADFinishSelection(s, nil, @"取词时前台应用已改变。") ; return; }
                    if (text.length) ADFinishSelection(s, text, nil);
                    else ADCopyFallback(s, pid, deadline);
                });
            }
        });
    });
}

void ad_select_text(void *context, ADCallback callback) {
    ad_select_text_for_pid(ad_selection_target_pid(), context, callback);
}

@class ADCaptureController;
@interface ADSelectionView : NSView
@property NSImage *image;
@property CGPoint anchor;
@property CGRect selected;
@property BOOL dragging;
@property (weak) ADCaptureController *controller;
@property NSScreen *screen;
@end

@interface ADCaptureWindow : NSWindow
@end
@implementation ADCaptureWindow
- (BOOL)canBecomeKeyWindow { return YES; }
- (BOOL)canBecomeMainWindow { return NO; }
@end

@interface ADCaptureController : NSObject
@property ADSession *session;
@property NSMutableArray<NSWindow *> *windows;
@property NSRunningApplication *previousApp;
@property id resignObserver;
@property BOOL finished;
- (void)finishView:(ADSelectionView *)view rect:(CGRect)rect;
- (void)cancel;
@end

static ADCaptureController *ADCaptureActive;

@implementation ADSelectionView
- (BOOL)acceptsFirstResponder { return YES; }
- (BOOL)isFlipped { return YES; }
- (void)resetCursorRects { [self addCursorRect:self.bounds cursor:NSCursor.crosshairCursor]; }
- (void)drawRect:(NSRect)dirty {
    [self.image drawInRect:self.bounds fromRect:NSZeroRect operation:NSCompositingOperationCopy fraction:1 respectFlipped:YES hints:nil];
    [[[NSColor blackColor] colorWithAlphaComponent:0.34] set];
    NSRectFillUsingOperation(self.bounds, NSCompositingOperationSourceOver);
    if (_dragging && _selected.size.width > 0 && _selected.size.height > 0) {
        [NSGraphicsContext saveGraphicsState];
        NSRectClip(_selected);
        [self.image drawInRect:self.bounds fromRect:NSZeroRect operation:NSCompositingOperationCopy fraction:1 respectFlipped:YES hints:nil];
        [NSGraphicsContext restoreGraphicsState];
        [NSColor.whiteColor set];
        NSBezierPath *path = [NSBezierPath bezierPathWithRect:_selected]; path.lineWidth = 1.5; [path stroke];
    }
    NSDictionary *attrs = @{NSFontAttributeName: [NSFont systemFontOfSize:13 weight:NSFontWeightMedium], NSForegroundColorAttributeName: NSColor.whiteColor};
    [(atomic_load(&ADUseEnglish) ? @"Drag to select text · Esc to cancel" : @"拖动框选文字 · Esc 取消") drawAtPoint:NSMakePoint(24, 22) withAttributes:attrs];
}
- (void)mouseDown:(NSEvent *)event { _anchor = [self convertPoint:event.locationInWindow fromView:nil]; _selected = CGRectZero; _dragging = YES; }
- (void)mouseDragged:(NSEvent *)event {
    CGPoint p = [self convertPoint:event.locationInWindow fromView:nil];
    p.x = fmin(fmax(0, p.x), self.bounds.size.width); p.y = fmin(fmax(0, p.y), self.bounds.size.height);
    _selected = CGRectMake(fmin(p.x, _anchor.x), fmin(p.y, _anchor.y), fabs(p.x-_anchor.x), fabs(p.y-_anchor.y));
    self.needsDisplay = YES;
}
- (void)mouseUp:(NSEvent *)event {
    [self mouseDragged:event];
    if (_selected.size.width >= 4 && _selected.size.height >= 4) [self.controller finishView:self rect:_selected];
    else { _dragging = NO; self.needsDisplay = YES; }
}
- (void)keyDown:(NSEvent *)event { if (event.keyCode == 53) [self.controller cancel]; else [super keyDown:event]; }
@end

static NSString *ADOCRImage(CGImageRef image, NSError **error) {
    VNRecognizeTextRequest *request = [VNRecognizeTextRequest new];
    request.recognitionLevel = VNRequestTextRecognitionLevelAccurate;
    request.recognitionLanguages = @[@"zh-Hans", @"en-US"];
    request.usesLanguageCorrection = YES;
    // Revision 3 fails repeat requests with CRImageReaderError on the test host.
    // Revision 2 passes repeated mixed Chinese/English fixtures; keep this
    // compatibility path explicit until revision 3 passes the same regression.
    request.revision = VNRecognizeTextRequestRevision2;
    VNImageRequestHandler *handler = [[VNImageRequestHandler alloc] initWithCGImage:image options:@{}];
    if (![handler performRequests:@[request] error:error]) return nil;
    NSMutableArray<NSString *> *lines = [NSMutableArray new];
    for (VNRecognizedTextObservation *obs in request.results) {
        VNRecognizedText *top = [obs topCandidates:1].firstObject;
        if (top.string.length) [lines addObject:top.string];
    }
    return [lines componentsJoinedByString:@"\n"];
}

static dispatch_queue_t ADOCRQueue(void) {
    static dispatch_queue_t queue;
    static dispatch_once_t once;
    dispatch_once(&once, ^{ queue = dispatch_queue_create("com.yurelay.ocr", DISPATCH_QUEUE_SERIAL); });
    return queue;
}

@implementation ADCaptureController
- (void)closeOverlays {
    if (self.resignObserver) { [NSNotificationCenter.defaultCenter removeObserver:self.resignObserver]; self.resignObserver = nil; }
    for (NSWindow *w in self.windows) { [w orderOut:nil]; w.contentView = nil; }
    [self.windows removeAllObjects];
    if (NSApp.active) [self.previousApp activateWithOptions:0];
    if (ADCaptureActive == self) ADCaptureActive = nil;
}
- (void)cancel {
    if (_finished) return; _finished = YES;
    [self closeOverlays]; [self.session send:4 data:nil]; [self.session end];
}
- (void)finishView:(ADSelectionView *)view rect:(CGRect)rect {
    if (_finished) return;
    CGImageRef full = [view.image CGImageForProposedRect:NULL context:nil hints:nil];
    if (!full) { [self.session text:3 value:@"屏幕图像已失效，请重新截图。"] ; [self cancel]; return; }
    CGFloat scaleX = (CGFloat)CGImageGetWidth(full)/view.bounds.size.width;
    CGFloat scaleY = (CGFloat)CGImageGetHeight(full)/view.bounds.size.height;
    CGRect crop = CGRectIntegral(CGRectMake(rect.origin.x*scaleX, rect.origin.y*scaleY, rect.size.width*scaleX, rect.size.height*scaleY));
    if (crop.size.width * crop.size.height > 16 * 1024 * 1024) {
        [self.session text:3 value:@"选区过大，请重新选择较小区域。"];
        return;
    }
    CGImageRef image = CGImageCreateWithImageInRect(full, crop);
    if (!image) { [self.session text:3 value:@"无法截取该区域，请重试。"] ; [self cancel]; return; }
    _finished = YES;
    ADSession *session = self.session;
    [session send:8 data:nil];
    [self closeOverlays];
    dispatch_group_t group = dispatch_group_create();
    dispatch_queue_t queue = dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0);
    dispatch_group_async(group, ADOCRQueue(), ^{
        @autoreleasepool {
            NSError *error = nil;
            NSString *text = ADOCRImage(image, &error);
            if (text.length) [session text:2 value:text];
            else [session text:6 value:error ? @"本地文字识别失败，请缩小选区后重试。" : @"选区中没有识别到文字。"];
        }
    });
    dispatch_group_async(group, queue, ^{
        @autoreleasepool {
            NSBitmapImageRep *rep = [[NSBitmapImageRep alloc] initWithCGImage:image];
            NSData *png = [rep representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
            if (png) [session send:1 data:png];
            else [session text:7 value:@"截图编码失败。"];
        }
    });
    dispatch_group_notify(group, queue, ^{ CGImageRelease(image); [session end]; });
}
@end

void ad_capture(void *context, ADCallback callback) {
    ADSession *session = ADMakeSession(context, callback);
    dispatch_async(dispatch_get_main_queue(), ^{
        if (ADCaptureActive) { [session text:3 value:@"正在截图，请完成或取消当前选区。"] ; [session end]; return; }
        if (!CGPreflightScreenCaptureAccess()) { [session text:3 value:@"请先在设置中开启 Yurelay 的屏幕录制权限。"] ; [session end]; return; }
        ADCaptureController *controller = [ADCaptureController new];
        controller.session = session; controller.windows = [NSMutableArray new];
        controller.previousApp = NSWorkspace.sharedWorkspace.frontmostApplication;
        ADCaptureActive = controller;
        [SCShareableContent getShareableContentExcludingDesktopWindows:NO onScreenWindowsOnly:YES completionHandler:^(SCShareableContent *content, NSError *error) {
            if (!content || error) {
                dispatch_async(dispatch_get_main_queue(), ^{ [session text:3 value:@"无法读取屏幕，请检查屏幕录制权限。"] ; [controller cancel]; });
                return;
            }
            dispatch_async(dispatch_get_main_queue(), ^{
                NSArray<NSScreen *> *screens = NSScreen.screens;
                dispatch_group_t snapshots = dispatch_group_create();
                NSMutableDictionary<NSNumber *, NSImage *> *images = [NSMutableDictionary new];
                for (NSScreen *screen in screens) {
                    NSNumber *number = screen.deviceDescription[@"NSScreenNumber"];
                    SCDisplay *display = nil;
                    for (SCDisplay *d in content.displays) if (d.displayID == number.unsignedIntValue) { display = d; break; }
                    if (!display) continue;
                    NSMutableArray<SCWindow *> *ownWindows = [NSMutableArray new];
                    for (SCWindow *w in content.windows) if (w.owningApplication.processID == getpid()) [ownWindows addObject:w];
                    SCContentFilter *filter = [[SCContentFilter alloc] initWithDisplay:display excludingWindows:ownWindows];
                    SCStreamConfiguration *config = [SCStreamConfiguration new];
                    config.width = (size_t)(screen.frame.size.width*screen.backingScaleFactor);
                    config.height = (size_t)(screen.frame.size.height*screen.backingScaleFactor);
                    config.showsCursor = NO;
                    config.capturesAudio = NO;
                    dispatch_group_enter(snapshots);
                    [SCScreenshotManager captureImageWithFilter:filter configuration:config completionHandler:^(CGImageRef image, NSError *captureError) {
                        if (image && !captureError) {
                            NSImage *value = [[NSImage alloc] initWithCGImage:image size:screen.frame.size];
                            @synchronized(images) { images[number] = value; }
                        }
                        dispatch_group_leave(snapshots);
                    }];
                }
                dispatch_group_notify(snapshots, dispatch_get_main_queue(), ^{
                    if (controller.finished) return;
                    if (!ADSameApp(controller.previousApp.processIdentifier)) { [controller cancel]; return; }
                    for (NSScreen *screen in screens) {
                        NSImage *image = images[screen.deviceDescription[@"NSScreenNumber"]];
                        if (!image) continue;
                        ADCaptureWindow *window = [[ADCaptureWindow alloc] initWithContentRect:screen.frame styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
                        window.level = NSScreenSaverWindowLevel;
                        window.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehaviorFullScreenAuxiliary;
                        window.opaque = YES; window.hasShadow = NO; window.releasedWhenClosed = NO;
                        ADSelectionView *view = [[ADSelectionView alloc] initWithFrame:NSMakeRect(0,0,screen.frame.size.width,screen.frame.size.height)];
                        view.image = image; view.screen = screen; view.controller = controller;
                        view.accessibilityLabel = (atomic_load(&ADUseEnglish) ? @"Screenshot selection. Drag to select text, press Escape to cancel." : @"截图选区，拖动鼠标框选文字，按 Escape 取消");
                        window.contentView = view;
                        [controller.windows addObject:window];
                        [window orderFrontRegardless];
                        if (NSPointInRect(NSEvent.mouseLocation, screen.frame)) { [window makeKeyWindow]; [window makeFirstResponder:view]; }
                    }
                    if (!controller.windows.count) { [session text:3 value:@"屏幕截图失败，请检查权限后重试。"] ; [controller cancel]; }
                    else {
                        [NSApp activateIgnoringOtherApps:YES];
                        __weak ADCaptureController *weakController = controller;
                        controller.resignObserver = [NSNotificationCenter.defaultCenter addObserverForName:NSApplicationDidResignActiveNotification object:NSApp queue:NSOperationQueue.mainQueue usingBlock:^(NSNotification *note) {
                            (void)note;
                            [weakController cancel];
                        }];
                    }
                });
            });
        }];
    });
}

// Local detection only. Uncertain / non-linguistic input deliberately falls
// through to translation; never suppress it merely because it uses Latin letters.
size_t ad_detect_language(const char *text, char *output, size_t capacity) {
    @autoreleasepool {
        NSString *value = [NSString stringWithUTF8String:text];
        if (!value.length || capacity < 16) return 0;
        NSUInteger hanLetters = 0, latinLetters = 0, letters = 0;
        for (NSUInteger index = 0; index < value.length; index++) {
            unichar c = [value characterAtIndex:index];
            hanLetters += (c >= 0x3400 && c <= 0x9fff) ? 1 : 0;
            latinLetters += ((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z')) ? 1 : 0;
            letters += [[NSCharacterSet letterCharacterSet] characterIsMember:c] ? 1 : 0;
        }
        if (letters < 2) return 0;
        if (hanLetters && latinLetters) {
            // Chinese technical prose often contains identifiers. Detect only the
            // surrounding prose; keep the actual query unchanged. A substantial
            // English phrase still needs translation rather than a local copy.
            NSRegularExpression *phrase = [NSRegularExpression regularExpressionWithPattern:@"[A-Za-z]+(?:[ \\t]+[A-Za-z]+){2,}" options:0 error:NULL];
            if (hanLetters < 8 || (double)hanLetters / letters < 0.55 ||
                [phrase firstMatchInString:value options:0 range:NSMakeRange(0, value.length)]) return 0;
            NSRegularExpression *identifier = [NSRegularExpression regularExpressionWithPattern:@"[A-Za-z][A-Za-z0-9_]*" options:0 error:NULL];
            value = [identifier stringByReplacingMatchesInString:value options:0 range:NSMakeRange(0, value.length) withTemplate:@" "];
        }
        NLTokenizer *tokenizer = [[NLTokenizer alloc] initWithUnit:NLTokenUnitWord];
        tokenizer.string = value;
        __block NSUInteger words = 0;
        [tokenizer enumerateTokensInRange:NSMakeRange(0, value.length) usingBlock:^(NSRange range, NLTokenizerAttributes attributes, BOOL *stop) { (void)range; (void)attributes; words++; *stop = YES; }];
        if (!words) return 0;
        NLLanguageRecognizer *recognizer = [NLLanguageRecognizer new];
        [recognizer processString:value];
        NLLanguage language = recognizer.dominantLanguage;
        if (!language) return 0;
        double confidence = [recognizer languageHypothesesWithMaximum:1][language].doubleValue;
        if (confidence < 0.8) return 0;
        NSData *data = [language dataUsingEncoding:NSUTF8StringEncoding];
        if (!data.length || data.length >= capacity) return 0;
        memcpy(output, data.bytes, data.length);
        return data.length;
    }
}

size_t ad_lemma(const char *input, unsigned char *output, size_t capacity) {
    @autoreleasepool {
        NSString *text = [NSString stringWithUTF8String:input];
        NLTagger *tagger = [[NLTagger alloc] initWithTagSchemes:@[NLTagSchemeLemma]];
        tagger.string = text;
        NSMutableArray *parts = [NSMutableArray array];
        [tagger enumerateTagsInRange:NSMakeRange(0,text.length) unit:NLTokenUnitWord scheme:NLTagSchemeLemma options:NLTaggerOmitWhitespace | NLTaggerOmitPunctuation usingBlock:^(NLTag tag, NSRange range, BOOL *stop) {
            (void)stop;
            [parts addObject:tag ?: [text substringWithRange:range]];
        }];
        NSString *lemma = parts.count ? [parts componentsJoinedByString:@" "] : text;
        NSData *data = [lemma.lowercaseString dataUsingEncoding:NSUTF8StringEncoding];
        if(data.length < capacity) memcpy(output,data.bytes,data.length);
        return MIN(data.length,capacity);
    }
}

void ad_choose_export(const char *name, void *context, ADCallback callback) {
    NSString *filename = [NSString stringWithUTF8String:name];
    ADSession *session = ADMakeSession(context,callback);
    dispatch_async(dispatch_get_main_queue(), ^{
        NSSavePanel *panel = [NSSavePanel savePanel];
        panel.nameFieldStringValue = filename;
        panel.title = atomic_load(&ADUseEnglish) ? @"Export vocabulary" : @"导出生词本";
        panel.canCreateDirectories = YES;
        [panel beginWithCompletionHandler:^(NSModalResponse response) {
            if(response == NSModalResponseOK && panel.URL.path) [session text:2 value:panel.URL.path];
            else [session send:4 data:nil];
            [session end];
        }];
    });
}
