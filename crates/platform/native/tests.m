// Native smoke test: uses the exact OCR helper compiled into the application.
// It only reads a synthetic in-memory fixture, never the screen or clipboard.
#import "macos.m"

int main(void) {
    @autoreleasepool {
        setvbuf(stdout, NULL, _IOLBF, 0);
        for (NSArray *fixture in @[@[@"running", @"run"], @[@"took off", @"take off"]]) {
            unsigned char bytes[2048]={0};size_t length=ad_lemma([fixture[0] UTF8String],bytes,sizeof(bytes));
            NSString *lemma=[[NSString alloc] initWithBytes:bytes length:length encoding:NSUTF8StringEncoding];
            BOOL passed=[lemma isEqualToString:fixture[1]];
            printf("Lemma %s -> %s passed=%s\n",[fixture[0] UTF8String],lemma.UTF8String,passed?"true":"false");
            if(!passed)return 1;
        }
        [NSApplication sharedApplication];
        for (NSArray *fixture in @[@[@"这是用于检查本地语言检测的中文句子。", @"zh"], @[@"This is an English sentence used to verify local language detection.", @"en"], @[@"Bonjour, ceci est une phrase en français pour vérifier la détection de la langue.", @"fr"]]) {
            char code[32] = {0};
            size_t len = ad_detect_language([fixture[0] UTF8String], code, sizeof(code));
            NSString *language = [[NSString alloc] initWithBytes:code length:len encoding:NSUTF8StringEncoding];
            BOOL passed = [language hasPrefix:fixture[1]];
            printf("Language expected=%s actual=%s passed=%s\n", [fixture[1] UTF8String], language.UTF8String ?: "none", passed ? "true" : "false");
            if (!passed) return 1;
        }
        const char *mixedChinese = "查到了：两个题目其实都有job，归属都是muse03，目前处于 needs_reconciliation（待核对恢复），租约分别在今晚22:27:47和22:28:28过期。\n所以不是单纯刷新版本号能解决的问题：7/8是原稿版本，和认领租约是两套状态。按题目查询、不限制status时能看到job，原先列表可能过滤掉了这种状态。我再确认恢复路径，不能直接当作无任务清锁。";
        char mixedCode[32] = {0};
        size_t mixedLen = ad_detect_language(mixedChinese, mixedCode, sizeof(mixedCode));
        NSString *mixedLanguage = [[NSString alloc] initWithBytes:mixedCode length:mixedLen encoding:NSUTF8StringEncoding];
        printf("Chinese with embedded identifiers actual=%s\n", mixedLanguage.UTF8String ?: "none");
        if (![mixedLanguage isEqualToString:@"zh-Hans"]) return 1;
        char uncertain[32] = {0};
        if (ad_detect_language("123456", uncertain, sizeof(uncertain)) != 0 ||
            ad_detect_language("你好，please translate this text.", uncertain, sizeof(uncertain)) != 0 ||
            ad_detect_language("这是中文正文，包含完整英文句子时仍需要翻译。Please translate the complete English sentence.", uncertain, sizeof(uncertain)) != 0) return 1;
        char traditional[32] = {0};
        size_t traditionalLen = ad_detect_language("這是用於檢查本地語言偵測的繁體中文句子，status 顯示任務已完成。", traditional, sizeof(traditional));
        if (![[[NSString alloc] initWithBytes:traditional length:traditionalLen encoding:NSUTF8StringEncoding] isEqualToString:@"zh-Hant"]) return 1;
        printf("Language uncertain numeric/mixed passed=true\n");
        unsigned char *definition = calloc(1024 * 1024, 1);
        size_t definitionLength = ad_dictionary_lookup("hello", definition, 1024 * 1024);
        printf("System dictionary available=%s bytes=%lu\n", definitionLength ? "true" : "false", (unsigned long)definitionLength);
        if (definitionLength >= 1024 * 1024 || (definitionLength && ![[NSString alloc] initWithBytes:definition length:definitionLength encoding:NSUTF8StringEncoding])) return 1;
        if (ad_dictionary_lookup("glossa_zxqv_nonexistent_dictionary_fixture", definition, 1024 * 1024) != 0) return 1;
        free(definition);
        NSBitmapImageRep *rep = [[NSBitmapImageRep alloc]
            initWithBitmapDataPlanes:NULL pixelsWide:1200 pixelsHigh:260
            bitsPerSample:8 samplesPerPixel:4 hasAlpha:YES isPlanar:NO
            colorSpaceName:NSDeviceRGBColorSpace bytesPerRow:0 bitsPerPixel:0];
        NSGraphicsContext *context = [NSGraphicsContext graphicsContextWithBitmapImageRep:rep];
        [NSGraphicsContext saveGraphicsState];
        NSGraphicsContext.currentContext = context;
        [NSColor.whiteColor set]; NSRectFill(NSMakeRect(0, 0, 1200, 260));
        NSDictionary *attributes = @{NSFontAttributeName: [NSFont systemFontOfSize:34], NSForegroundColorAttributeName: NSColor.blackColor};
        [@"Glossa keeps translation tasks running." drawAtPoint:NSMakePoint(40, 150) withAttributes:attributes];
        [@"切换窗口后，翻译结果仍然保留。" drawAtPoint:NSMakePoint(40, 70) withAttributes:attributes];
        [NSGraphicsContext restoreGraphicsState];

        CGImageRef fixture = CGImageCreateCopy(rep.CGImage);
        dispatch_async(ADOCRQueue(), ^{
        for (int run = 0; run < 12; run++) {
          @autoreleasepool {
            NSError *error = nil;
            CFAbsoluteTime begin = CFAbsoluteTimeGetCurrent();
            NSString *text = ADOCRImage(fixture, &error);
            BOOL passed = !error && [text containsString:@"Glossa"] && [text containsString:@"翻译结果"];
            printf("OCR run=%d passed=%s elapsed_ms=%.1f characters=%lu\n", run, passed ? "true" : "false", (CFAbsoluteTimeGetCurrent()-begin)*1000, (unsigned long)text.length);
            if (!passed) {
                fprintf(stderr, "OCR failure domain=%s code=%ld description=%s text=%s\n", error.domain.UTF8String ?: "none", (long)error.code, error.localizedDescription.UTF8String ?: "none", text.UTF8String ?: "nil");
                exit(1);
            }
          }
        }
        CGImageRelease(fixture);
        exit(0);
        });
        [NSApp run];
        return 0;
    }
}
