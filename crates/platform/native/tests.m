// Native smoke tests use the exact helpers compiled into the application.
// Clipboard cases use fake objects or a uniquely named synthetic pasteboard,
// never the general clipboard; OCR uses an in-memory synthetic image.
#import "macos.m"
#import "ocr_pipeline_cases.m"


// NSPasteboard is a class cluster that refuses subclass allocation. This fake
// implements only the read-only selectors consumed by the backup helper.
@interface ADBackupTestPasteboard : NSObject
@property(nonatomic, copy) NSArray<NSPasteboardItem *> *fixtureItems;
@property(nonatomic) NSInteger fixtureCount;
@property(nonatomic, readonly) NSInteger changeCount;
@property(nonatomic, readonly, copy) NSArray<NSPasteboardItem *> *pasteboardItems;
@end
@implementation ADBackupTestPasteboard
- (NSArray<NSPasteboardItem *> *)pasteboardItems { return self.fixtureItems; }
- (NSInteger)changeCount { return self.fixtureCount; }
@end

@interface ADBackupTestItem : NSPasteboardItem
@property(nonatomic, copy) NSArray<NSPasteboardType> *fixtureTypes;
@property(nonatomic, copy) NSDictionary<NSPasteboardType, NSData *> *fixtureData;
@property(nonatomic, copy) void (^onDataRead)(void);
@end
@implementation ADBackupTestItem
- (NSArray<NSPasteboardType> *)types { return self.fixtureTypes; }
- (NSData *)dataForType:(NSPasteboardType)type {
    if (self.onDataRead) self.onDataRead();
    return self.fixtureData[type];
}
@end

static ADBackupTestItem *ADBackupFakeItem(NSArray<NSPasteboardType> *types,
                                         NSDictionary<NSPasteboardType, NSData *> *data) {
    ADBackupTestItem *item = [ADBackupTestItem new];
    item.fixtureTypes = types;
    item.fixtureData = data;
    return item;
}

static ADBackupTestPasteboard *ADBackupFakeBoard(NSArray<NSPasteboardItem *> *items) {
    ADBackupTestPasteboard *board = [ADBackupTestPasteboard new];
    board.fixtureItems = items;
    board.fixtureCount = 17;
    return board;
}

static void ADBackupCheck(const char *name, BOOL passed, BOOL *allPassed) {
    printf("Pasteboard backup %s passed=%s\n", name, passed ? "true" : "false");
    if (!passed) *allPassed = NO;
}

static BOOL ADBackupMatches(NSArray<NSPasteboardItem *> *items,
                            NSArray<NSDictionary<NSPasteboardType, NSData *> *> *expected) {
    if (!items || items.count != expected.count) return NO;
    for (NSUInteger i = 0; i < expected.count; i++) {
        NSPasteboardItem *item = items[i];
        NSDictionary<NSPasteboardType, NSData *> *formats = expected[i];
        if (item.types.count != formats.count) return NO;
        for (NSPasteboardType type in formats) {
            NSData *actual = [item dataForType:type];
            if (!actual || ![actual isEqualToData:formats[type]]) return NO;
        }
    }
    return YES;
}

static BOOL ADTestPasteboardBackup(void) {
    BOOL passed = YES;
    NSString *error = @"stale fixture error";
    ADBackupTestPasteboard *board = ADBackupFakeBoard(@[]);
    NSArray<NSPasteboardItem *> *backup = ADBackupPasteboard((NSPasteboard *)board, board.changeCount, &error);
    ADBackupCheck("empty-array", backup != nil && backup.count == 0 && error == nil, &passed);

    board = ADBackupFakeBoard(nil);
    backup = ADBackupPasteboard((NSPasteboard *)board, board.changeCount, &error);
    ADBackupCheck("unavailable-items", backup == nil && [error isEqualToString:@"无法读取当前剪贴板，已保留原内容。请重试或使用截图翻译。"], &passed);

    NSPasteboardType firstType = @"org.yurelay.synthetic.first";
    NSPasteboardType secondType = @"org.yurelay.synthetic.second";
    ADBackupTestItem *item = ADBackupFakeItem(@[firstType], @{});
    board = ADBackupFakeBoard(@[item]);
    backup = ADBackupPasteboard((NSPasteboard *)board, board.changeCount, &error);
    ADBackupCheck("unreadable-format", backup == nil && [error isEqualToString:@"剪贴板部分格式无法读取，已保留原内容。请使用截图翻译。"], &passed);

    NSData *readableData = [@"Synthetic race fixture" dataUsingEncoding:NSUTF8StringEncoding];
    __block BOOL staleFormatRead = NO;
    item = ADBackupFakeItem(@[firstType], @{firstType: readableData});
    item.onDataRead = ^{ staleFormatRead = YES; };
    board = ADBackupFakeBoard(@[item]);
    backup = ADBackupPasteboard((NSPasteboard *)board, board.changeCount - 1, &error);
    ADBackupCheck("stale-count-before-format-read", backup == nil && !staleFormatRead && [error isEqualToString:@"剪贴板或前台应用已改变，请重试。"], &passed);

    item = ADBackupFakeItem(@[firstType], @{});
    board = ADBackupFakeBoard(@[item]);
    __weak ADBackupTestPasteboard *raceBoard = board;
    item.onDataRead = ^{ raceBoard.fixtureCount++; };
    backup = ADBackupPasteboard((NSPasteboard *)board, board.changeCount, &error);
    ADBackupCheck("change-during-nil-read", backup == nil && [error isEqualToString:@"剪贴板或前台应用已改变，请重试。"], &passed);

    __block BOOL validFormatRead = NO;
    item = ADBackupFakeItem(@[firstType], @{firstType: readableData});
    board = ADBackupFakeBoard(@[item]);
    __weak ADBackupTestPasteboard *validRaceBoard = board;
    item.onDataRead = ^{ validFormatRead = YES; validRaceBoard.fixtureCount++; };
    backup = ADBackupPasteboard((NSPasteboard *)board, board.changeCount, &error);
    ADBackupCheck("change-during-valid-read", backup == nil && validFormatRead && [error isEqualToString:@"剪贴板或前台应用已改变，请重试。"], &passed);

    NSData *emptyData = [NSData data];
    item = ADBackupFakeItem(@[firstType], @{firstType: emptyData});
    board = ADBackupFakeBoard(@[item]);
    backup = ADBackupPasteboard((NSPasteboard *)board, board.changeCount, &error);
    ADBackupCheck("zero-byte-format", error == nil && ADBackupMatches(backup, @[@{firstType: emptyData}]), &passed);

    const NSUInteger limit = 8 * 1024 * 1024;
    NSString *sizeError = @"剪贴板内容超过 8 MiB，已保留原内容。请使用截图翻译。";
    NSData *oversized = [NSMutableData dataWithLength:limit + 1];
    item = ADBackupFakeItem(@[firstType], @{firstType: oversized});
    board = ADBackupFakeBoard(@[item]);
    backup = ADBackupPasteboard((NSPasteboard *)board, board.changeCount, &error);
    ADBackupCheck("over-eight-MiB", backup == nil && [error isEqualToString:sizeError], &passed);

    NSData *halfPlusOne = [NSMutableData dataWithLength:limit / 2 + 1];
    item = ADBackupFakeItem(@[firstType, secondType], @{firstType: halfPlusOne, secondType: halfPlusOne});
    board = ADBackupFakeBoard(@[item]);
    backup = ADBackupPasteboard((NSPasteboard *)board, board.changeCount, &error);
    ADBackupCheck("formats-cumulative-size", backup == nil && [error isEqualToString:sizeError], &passed);

    board = ADBackupFakeBoard(@[
        ADBackupFakeItem(@[firstType], @{firstType: halfPlusOne}),
        ADBackupFakeItem(@[secondType], @{secondType: halfPlusOne}),
    ]);
    backup = ADBackupPasteboard((NSPasteboard *)board, board.changeCount, &error);
    ADBackupCheck("items-cumulative-size", backup == nil && [error isEqualToString:sizeError], &passed);

    __block BOOL promiseRead = NO;
    NSPasteboardType promiseType = @"com.apple.pasteboard.promised-file-url";
    item = ADBackupFakeItem(@[promiseType], @{promiseType: emptyData});
    item.onDataRead = ^{ promiseRead = YES; };
    board = ADBackupFakeBoard(@[item]);
    backup = ADBackupPasteboard((NSPasteboard *)board, board.changeCount, &error);
    ADBackupCheck("promise-preserved-without-read", backup == nil && !promiseRead && [error isEqualToString:@"剪贴板含延迟数据，已保留剪贴板。请使用截图翻译。"], &passed);

    NSData *exactLimit = [NSMutableData dataWithLength:limit];
    item = ADBackupFakeItem(@[firstType], @{firstType: exactLimit});
    board = ADBackupFakeBoard(@[item]);
    backup = ADBackupPasteboard((NSPasteboard *)board, board.changeCount, &error);
    ADBackupCheck("exactly-eight-MiB", error == nil && ADBackupMatches(backup, @[@{firstType: exactLimit}]), &passed);

    // Exercise AppKit serialization and restoration only on a uniquely named board.
    NSPasteboard *named = [NSPasteboard pasteboardWithUniqueName];
    @try {
        [named clearContents];
        NSArray<NSPasteboardItem *> *emptyItems = named.pasteboardItems;
        backup = ADBackupPasteboard(named, named.changeCount, &error);
        printf("Pasteboard named-empty items_nil=%s count=%lu\n", emptyItems == nil ? "true" : "false", (unsigned long)emptyItems.count);
        ADBackupCheck("named-empty-pasteboard", backup != nil && backup.count == 0 && error == nil, &passed);
        const unsigned char syntheticBytes[] = {0x00, 0xff, 0x47, 0x6c};
        NSArray<NSDictionary<NSPasteboardType, NSData *> *> *formats = @[
            @{NSPasteboardTypeString: [@"Synthetic Yurelay source" dataUsingEncoding:NSUTF8StringEncoding],
              NSPasteboardTypeHTML: [@"<b>Synthetic Yurelay source</b>" dataUsingEncoding:NSUTF8StringEncoding],
              firstType: [NSData dataWithBytes:syntheticBytes length:sizeof(syntheticBytes)]},
            @{NSPasteboardTypeString: [@"Second synthetic item" dataUsingEncoding:NSUTF8StringEncoding],
              secondType: emptyData},
        ];
        NSMutableArray<NSPasteboardItem *> *source = [NSMutableArray new];
        for (NSDictionary<NSPasteboardType, NSData *> *itemFormats in formats) {
            NSPasteboardItem *sourceItem = [NSPasteboardItem new];
            for (NSPasteboardType type in itemFormats) [sourceItem setData:itemFormats[type] forType:type];
            [source addObject:sourceItem];
        }
        [named clearContents];
        BOOL wrote = [named writeObjects:source];
        NSInteger originalCount = named.changeCount;
        backup = ADBackupPasteboard(named, originalCount, &error);
        ADBackupCheck("named-multi-format-backup", wrote && error == nil && named.changeCount == originalCount && ADBackupMatches(backup, formats), &passed);
        [named clearContents];
        BOOL copied = [named setString:@"Synthetic selected text" forType:NSPasteboardTypeString];
        ADBackupCheck("named-simulated-copy", copied && named.changeCount != originalCount && [[named stringForType:NSPasteboardTypeString] isEqualToString:@"Synthetic selected text"], &passed);
        [named clearContents];
        BOOL restored = backup != nil && [named writeObjects:backup];
        ADBackupCheck("named-multi-format-restored", restored && ADBackupMatches(named.pasteboardItems, formats), &passed);
    } @finally {
        [named releaseGlobally];
    }
    return passed;
}

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
        if (getenv("YURELAY_OCR_MATRIX")) {
            dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED,0), ^{ exit(ADRunOCRPipelineMatrix()); });
            [NSApp run]; return 0;
        }
        if (!ADTestPasteboardBackup()) return 1;
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
        unsigned char ranges[16384] = {0};
        size_t rangeLength = ad_token_ranges("😀 running with friends", ranges, sizeof(ranges));
        NSArray *tokens = [NSJSONSerialization JSONObjectWithData:[NSData dataWithBytes:ranges length:rangeLength] options:0 error:nil];
        BOOL matchedRange = NO;
        for (NSDictionary *token in tokens) if ([token[@"surface"] isEqualToString:@"running"]) matchedRange = [token[@"start_utf16"] intValue] == 3 && [token[@"end_utf16"] intValue] == 10 && [token[@"lemma"] isEqualToString:@"run"];
        if (!matchedRange) return 1;
        printf("Local token UTF16 and lemma passed=true\n");
        unsigned char *definition = calloc(1024 * 1024, 1);
        size_t definitionLength = ad_dictionary_lookup("hello", definition, 1024 * 1024);
        printf("System dictionary available=%s bytes=%lu\n", definitionLength ? "true" : "false", (unsigned long)definitionLength);
        if (definitionLength >= 1024 * 1024 || (definitionLength && ![[NSString alloc] initWithBytes:definition length:definitionLength encoding:NSUTF8StringEncoding])) return 1;
        if (ad_dictionary_lookup("yurelay_zxqv_nonexistent_dictionary_fixture", definition, 1024 * 1024) != 0) return 1;
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
        [@"Yurelay keeps translation tasks running." drawAtPoint:NSMakePoint(40, 150) withAttributes:attributes];
        [@"切换窗口后，翻译结果仍然保留。" drawAtPoint:NSMakePoint(40, 70) withAttributes:attributes];
        [NSGraphicsContext restoreGraphicsState];

        CGImageRef fixture = CGImageCreateCopy(rep.CGImage);
        dispatch_async(ADOCRQueue(), ^{
        for (int run = 0; run < 12; run++) {
          @autoreleasepool {
            NSError *error = nil;
            CFAbsoluteTime begin = CFAbsoluteTimeGetCurrent();
            NSData *layoutData = nil;
            NSString *text = ADOCRImageWithLayout(fixture, &error, &layoutData);
            NSDictionary *layout = [NSJSONSerialization JSONObjectWithData:layoutData options:0 error:nil];
            if ([layout[@"width"] intValue] != 1200 || [layout[@"height"] intValue] != 260 || [layout[@"blocks"] count] < 2) exit(1);
            for (NSDictionary *block in layout[@"blocks"]) {
                for (NSString *key in @[@"x",@"y",@"width",@"height",@"confidence"]) {double value=[block[key] doubleValue];if (!isfinite(value)||value<0||value>1) exit(1);}
            }
            if (run == 0) {
                CGImageRef crop=CGImageCreateWithImageInRect(fixture,CGRectMake(0,0,1200,130));
                NSData *cropData=nil;NSError *cropError=nil;NSString *cropText=ADOCRImageWithLayout(crop,&cropError,&cropData);CGImageRelease(crop);
                NSDictionary *cropLayout=[NSJSONSerialization JSONObjectWithData:cropData options:0 error:nil];
                if(cropError||!cropText.length||[cropLayout[@"height"] intValue]!=130) exit(1);
                CGColorSpaceRef color=CGColorSpaceCreateDeviceRGB();CGContextRef scaledContext=CGBitmapContextCreate(NULL,600,130,8,0,color,(CGBitmapInfo)kCGImageAlphaPremultipliedLast);CGColorSpaceRelease(color);CGContextDrawImage(scaledContext,CGRectMake(0,0,600,130),fixture);CGImageRef scaled=CGBitmapContextCreateImage(scaledContext);CGContextRelease(scaledContext);
                NSData *scaledData=nil;NSError *scaledError=nil;NSString *scaledText=ADOCRImageWithLayout(scaled,&scaledError,&scaledData);CGImageRelease(scaled);
                NSDictionary *scaledLayout=[NSJSONSerialization JSONObjectWithData:scaledData options:0 error:nil];
                if(scaledError||!scaledText.length||[scaledLayout[@"width"] intValue]!=600||[scaledLayout[@"height"] intValue]!=130) exit(1);
                printf("OCR crop/scale layout bounds passed=true\n");
            }
            BOOL passed = !error && [text containsString:@"Yurelay"] && [text containsString:@"翻译结果"];
            printf("OCR run=%d passed=%s elapsed_ms=%.1f characters=%lu\n", run, passed ? "true" : "false", (CFAbsoluteTimeGetCurrent()-begin)*1000, (unsigned long)text.length);
            if (!passed) {
                fprintf(stderr, "OCR failure domain=%s code=%ld\n", error.domain.UTF8String ?: "none", (long)error.code);
                exit(1);
            }
          }
        }
        CGImageRelease(fixture);
        dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED,0), ^{ exit(ADRunOCRPipelineMatrix()); });
        });
        [NSApp run];
        return 0;
    }
}
