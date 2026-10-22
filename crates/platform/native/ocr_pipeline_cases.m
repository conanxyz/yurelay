// Synthetic capture pipeline probes. Included only by tests.m, never the app.
// Every image is generated in memory; no screen, window or clipboard is read.
#import <unistd.h>
#import <stdint.h>

// Synthetic-only error labels keep diagnostics bounded even in test output.
static NSString *ADTestOCRFailureCategory(NSError *error) {
    if ([error.domain isEqualToString:VNErrorDomain]) return @"vision";
    if ([error.domain isEqualToString:@"TextRecognition.CRImageReaderError"]) return @"image_reader";
    return @"other";
}

@interface ADPipelineEvents : NSObject
@property dispatch_semaphore_t finished;
@property NSUInteger textEvents;
@property NSUInteger imageEvents;
@property NSUInteger layoutEvents;
@property NSUInteger ocrErrors;
@property NSUInteger noTextResults;
@property NSUInteger imageErrors;
@property NSUInteger otherErrors;
@property NSUInteger ends;
@end
@implementation ADPipelineEvents
@end

static void ADPipelineReceive(void *context, int tag, const unsigned char *bytes, size_t length) {
    ADPipelineEvents *events = (__bridge ADPipelineEvents *)context;
    switch (tag) {
        case 1: if (length) events.imageEvents++; break;
        case 2: if (length) events.textEvents++; break;
        case 9: if (length) events.layoutEvents++; break;
        case 6: {
            NSData *emptyMessage=[@"选区中没有识别到文字。" dataUsingEncoding:NSUTF8StringEncoding];
            if(length==emptyMessage.length&&memcmp(bytes,emptyMessage.bytes,length)==0)events.noTextResults++;else events.ocrErrors++;
            break;
        }
        case 7: events.imageErrors++; break;
        case 3: case 4: events.otherErrors++; break;
        case 5: events.ends++; dispatch_semaphore_signal(events.finished); break;
        default: break;
    }
}

typedef struct {
    unsigned char *bytes;
    size_t length;
    atomic_uint reads;
    BOOL delayed;
} ADProbeProvider;
static void ADProbeDelay(ADProbeProvider *provider) {
    if (atomic_fetch_add(&provider->reads, 1) == 0 && provider->delayed) usleep(25000);
}
static const void *ADProbePointer(void *info) { ADProbeProvider *p=info; ADProbeDelay(p); return p->bytes; }
static void ADProbeReleasePointer(void *info, const void *pointer) { (void)info; (void)pointer; }
static size_t ADProbeBytes(void *info, void *buffer, off_t position, size_t count) {
    ADProbeProvider *p=info; ADProbeDelay(p);
    if (position<0 || (size_t)position>=p->length) return 0;
    count=MIN(count,p->length-(size_t)position); memcpy(buffer,p->bytes+(size_t)position,count); return count;
}
static void ADProbeRelease(void *info) { ADProbeProvider *p=info; free(p->bytes); free(p); }

static CGImageRef ADCreatePipelineFixture(size_t width, size_t height, NSString *format, CGFloat fontSize, BOOL delayed) CF_RETURNS_RETAINED {
    CGColorSpaceRef rgb=CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    CGContextRef drawing=CGBitmapContextCreate(NULL,width,height,8,width*4,rgb,(CGBitmapInfo)kCGImageAlphaPremultipliedLast|kCGBitmapByteOrder32Big);
    CGColorSpaceRelease(rgb);
    if (!drawing) return NULL;
    [NSGraphicsContext saveGraphicsState];
    NSGraphicsContext.currentContext=[NSGraphicsContext graphicsContextWithCGContext:drawing flipped:NO];
    [NSColor.whiteColor set];NSRectFill(NSMakeRect(0,0,width,height));
    CGFloat title=MIN(fontSize,(CGFloat)height/5.0);
    NSDictionary *large=@{NSFontAttributeName:[NSFont systemFontOfSize:title weight:NSFontWeightSemibold],NSForegroundColorAttributeName:NSColor.blackColor};
    NSDictionary *normal=@{NSFontAttributeName:[NSFont systemFontOfSize:MAX(16,title*.55)],NSForegroundColorAttributeName:NSColor.blackColor};
    [@"Yurelay capture check" drawAtPoint:NSMakePoint(45,height-title-35) withAttributes:large];
    [@"Synthetic text on two displays." drawAtPoint:NSMakePoint(45,height-title*2.05-35) withAttributes:normal];
    [@"本地识别测试，原文仅使用合成资料。" drawAtPoint:NSMakePoint(45,height-title*3.0-35) withAttributes:normal];
    [NSGraphicsContext restoreGraphicsState];
    unsigned char *source=CGBitmapContextGetData(drawing);
    size_t bits=8;
    BOOL floating=[format containsString:@"float"];
    if ([format containsString:@"16"]) bits=16;
    if ([format containsString:@"32"]) bits=32;
    // Odd source widths and a non-aligned crop deliberately preserve uncommon strides.
    size_t rowBytes=width*4*(bits/8);
    if([format containsString:@"padded"])rowBytes=(rowBytes+255)&~(size_t)255;
    ADProbeProvider *provider=calloc(1,sizeof(*provider));
    provider->length=rowBytes*height; provider->bytes=calloc(1,provider->length);provider->delayed=delayed;
    atomic_init(&provider->reads,0);
    if (!provider->bytes) {free(provider);CGContextRelease(drawing);return NULL;}
    for(size_t pixel=0;pixel<width*height;pixel++)for(size_t component=0;component<4;component++){
        unsigned char value=source[pixel*4+component];size_t offset=(pixel/width)*rowBytes+((pixel%width)*4+component)*(bits/8);
        if(bits==8)provider->bytes[offset]=value;
        else if(bits==16&&floating){_Float16 half=(_Float16)((float)value/255.0f);memcpy(provider->bytes+offset,&half,2);}
        else if(bits==16){uint16_t integer=(uint16_t)value*257;memcpy(provider->bytes+offset,&integer,2);}
        else {float real=(float)value/255.0f;memcpy(provider->bytes+offset,&real,4);}
    }
    CGContextRelease(drawing);
    CFStringRef colorName=[format containsString:@"p3"]?kCGColorSpaceDisplayP3:([format containsString:@"extended"]?kCGColorSpaceExtendedSRGB:kCGColorSpaceSRGB);
    CGColorSpaceRef color=CGColorSpaceCreateWithName(colorName);
    CGBitmapInfo bitmap=(CGBitmapInfo)kCGImageAlphaPremultipliedLast;
    if(bits==8)bitmap|=kCGBitmapByteOrder32Big;
    else if(bits==16)bitmap|=kCGBitmapByteOrder16Little;
    else bitmap|=kCGBitmapByteOrder32Little;
    if(floating)bitmap|=kCGBitmapFloatComponents;
    if([format containsString:@"bgra8"]){bitmap=(CGBitmapInfo)kCGImageAlphaPremultipliedFirst|kCGBitmapByteOrder32Little;}
    if([format containsString:@"bgrx8"])bitmap=(CGBitmapInfo)kCGImageAlphaNoneSkipFirst|kCGBitmapByteOrder32Little;
    CGDataProviderDirectCallbacks callbacks={0,ADProbePointer,ADProbeReleasePointer,ADProbeBytes,ADProbeRelease};
    CGDataProviderRef data=CGDataProviderCreateDirect(provider,provider->length,&callbacks);
    CGImageRef image=CGImageCreate(width,height,bits,bits*4,rowBytes,color,bitmap,data,NULL,NO,kCGRenderingIntentDefault);
    CGDataProviderRelease(data);CGColorSpaceRelease(color);
    return image;
}

static void ADPrintPipelineImage(const char *label, CGImageRef image) {
    CFStringRef color=CGColorSpaceCopyName(CGImageGetColorSpace(image));
    char name[160]={0};if(color){CFStringGetCString(color,name,sizeof(name),kCFStringEncodingUTF8);CFRelease(color);}
    printf("OCR pipeline image stage=%s width=%zu height=%zu bpc=%zu bpp=%zu stride=%zu bitmap=0x%x colorspace=%s\n",label,CGImageGetWidth(image),CGImageGetHeight(image),CGImageGetBitsPerComponent(image),CGImageGetBitsPerPixel(image),CGImageGetBytesPerRow(image),(unsigned)CGImageGetBitmapInfo(image),name);
}

static BOOL ADProbePipelineCase(NSString *name, CGImageRef fixture, CGFloat scale, CGRect crop, BOOL allowEmpty) {
    if(!fixture){printf("OCR pipeline case=%s setup=false\n",name.UTF8String);return NO;}
    __block CGImageRef cropped=NULL;
    dispatch_sync(dispatch_get_main_queue(),^{
        @autoreleasepool {
            NSImage *image=[[NSImage alloc] initWithCGImage:fixture size:NSMakeSize(CGImageGetWidth(fixture)/scale,CGImageGetHeight(fixture)/scale)];
            CGImageRef full=[image CGImageForProposedRect:NULL context:nil hints:nil];
            ADPrintPipelineImage("nsimage",full);
            CGFloat scaleX=(CGFloat)CGImageGetWidth(full)/image.size.width,scaleY=(CGFloat)CGImageGetHeight(full)/image.size.height;
            CGRect pixels=CGRectIntegral(CGRectMake(crop.origin.x*scaleX,crop.origin.y*scaleY,crop.size.width*scaleX,crop.size.height*scaleY));
            cropped=CGImageCreateWithImageInRect(full,pixels);
        }
    });
    if(!cropped){printf("OCR pipeline case=%s crop=false\n",name.UTF8String);return NO;}
    ADPrintPipelineImage("crop",cropped);
    dispatch_group_t group=dispatch_group_create();__block NSError *ocrError=nil;__block NSUInteger textLength=0,pngBytes=0;__block BOOL hasLayout=NO;
    CFAbsoluteTime start=CFAbsoluteTimeGetCurrent();
    dispatch_group_async(group,ADOCRQueue(),^{@autoreleasepool{NSData *layout=nil;NSString *text=ADOCRImageWithLayout(cropped,&ocrError,&layout);textLength=text.length;hasLayout=layout!=nil;}});
    dispatch_group_async(group,dispatch_get_global_queue(QOS_CLASS_USER_INITIATED,0),^{@autoreleasepool{NSBitmapImageRep *rep=[[NSBitmapImageRep alloc] initWithCGImage:cropped];pngBytes=[rep representationUsingType:NSBitmapImageFileTypePNG properties:@{}].length;}});
    BOOL timedOut=dispatch_group_wait(group,dispatch_time(DISPATCH_TIME_NOW,30*NSEC_PER_SEC))!=0;
    if(timedOut){printf("OCR pipeline case=%s timeout=true\n",name.UTF8String);exit(2);}
    printf("OCR pipeline case=%s error_category=%s error_code=%ld text_present=%s layout=%s png_bytes=%lu elapsed_ms=%.1f\n",name.UTF8String,ocrError?ADTestOCRFailureCategory(ocrError).UTF8String:"none",(long)ocrError.code,textLength?"true":"false",hasLayout?"true":"false",(unsigned long)pngBytes,(CFAbsoluteTimeGetCurrent()-start)*1000);
    NSError *underlying=ocrError.userInfo[NSUnderlyingErrorKey];
    if(underlying)printf("OCR pipeline underlying case=%s category=%s code=%ld\n",name.UTF8String,ADTestOCRFailureCategory(underlying).UTF8String,(long)underlying.code);
    BOOL passed=!ocrError&&(allowEmpty||textLength>0)&&hasLayout&&pngBytes>0;
    CGImageRef actualFixture=CGImageRetain(fixture);
    CGRect actualCrop=crop;
    CGImageRelease(cropped);
    // Invoke the real desktop capture completion on a synthetic selection view.
    ADPipelineEvents *events=[ADPipelineEvents new];events.finished=dispatch_semaphore_create(0);
    dispatch_sync(dispatch_get_main_queue(),^{
        ADSelectionView *view=[[ADSelectionView alloc] initWithFrame:NSMakeRect(0,0,CGImageGetWidth(actualFixture)/scale,CGImageGetHeight(actualFixture)/scale)];
        view.image=[[NSImage alloc] initWithCGImage:actualFixture size:view.bounds.size];
        ADCaptureController *controller=[ADCaptureController new];controller.windows=[NSMutableArray new];controller.session=ADMakeSession((__bridge void *)events,ADPipelineReceive);view.controller=controller;
        [controller finishView:view rect:actualCrop];
        view.image=nil;
    });
    if(dispatch_semaphore_wait(events.finished,dispatch_time(DISPATCH_TIME_NOW,30*NSEC_PER_SEC))!=0){printf("OCR finishView case=%s timeout=true\n",name.UTF8String);exit(2);}
    BOOL recognized=events.textEvents==1&&events.layoutEvents==1&&events.noTextResults==0;
    BOOL empty=allowEmpty&&events.textEvents==0&&events.layoutEvents==0&&events.noTextResults==1;
    BOOL actual=(recognized||empty)&&events.imageEvents==1&&events.ocrErrors==0&&events.imageErrors==0&&events.otherErrors==0&&events.ends==1;
    printf("OCR finishView case=%s passed=%s text_events=%lu image_events=%lu ocr_errors=%lu no_text=%lu image_errors=%lu ends=%lu\n",name.UTF8String,actual?"true":"false",(unsigned long)events.textEvents,(unsigned long)events.imageEvents,(unsigned long)events.ocrErrors,(unsigned long)events.noTextResults,(unsigned long)events.imageErrors,(unsigned long)events.ends);
    CGImageRelease(actualFixture);
    return passed&&actual;
}

static int ADRunOCRPipelineMatrix(void) {
    NSArray<NSDictionary *> *cases=@[
        @{@"name":@"small-rgba8",@"width":@1200,@"height":@360,@"format":@"rgba8",@"scale":@1,@"font":@44},
        @{@"name":@"odd-bgra8-crop",@"width":@2117,@"height":@941,@"format":@"bgra8",@"scale":@2,@"font":@80},
        @{@"name":@"p3-bgra8-crop",@"width":@2117,@"height":@941,@"format":@"bgra8-p3",@"scale":@2,@"font":@80},
        @{@"name":@"large-p3-bgra8",@"width":@4096,@"height":@3072,@"format":@"bgra8-p3",@"scale":@2,@"font":@120},
        @{@"name":@"wide-p3-bgra8",@"width":@3840,@"height":@400,@"format":@"bgra8-p3",@"scale":@2,@"font":@64},
        @{@"name":@"large-title-p3",@"width":@3200,@"height":@1400,@"format":@"bgra8-p3",@"scale":@2,@"font":@220},
        @{@"name":@"extended-rgba8",@"width":@2117,@"height":@941,@"format":@"extended-rgba8",@"scale":@2,@"font":@80},
        @{@"name":@"p3-uint16",@"width":@1600,@"height":@700,@"format":@"p3-uint16",@"scale":@2,@"font":@80},
        @{@"name":@"p3-float16",@"width":@1600,@"height":@700,@"format":@"p3-float16",@"scale":@2,@"font":@80},
        @{@"name":@"extended-float32",@"width":@1600,@"height":@700,@"format":@"extended-float32",@"scale":@2,@"font":@80},
        @{@"name":@"padded-p3-bgra8",@"width":@2117,@"height":@941,@"format":@"padded-bgra8-p3",@"scale":@2,@"font":@80},
        @{@"name":@"padded-p3-bgrx8",@"width":@2117,@"height":@941,@"format":@"padded-bgrx8-p3",@"scale":@2,@"font":@80},
        @{@"name":@"thin-wide-p3",@"width":@7680,@"height":@128,@"format":@"bgra8-p3",@"scale":@2,@"font":@24,@"allowEmpty":@YES},
        @{@"name":@"tall-p3",@"width":@650,@"height":@7000,@"format":@"bgra8-p3",@"scale":@2,@"font":@44},
        @{@"name":@"delayed-p3-bgra8",@"width":@2117,@"height":@941,@"format":@"bgra8-p3",@"scale":@2,@"font":@80,@"delayed":@YES},
        @{@"name":@"pixel-4x4",@"width":@4,@"height":@4,@"format":@"bgra8",@"scale":@1,@"font":@24,@"allowEmpty":@YES,@"fullCrop":@YES},
        @{@"name":@"pixel-8x16",@"width":@8,@"height":@16,@"format":@"bgra8",@"scale":@2,@"font":@24,@"allowEmpty":@YES,@"fullCrop":@YES},
        @{@"name":@"pixel-16x16",@"width":@16,@"height":@16,@"format":@"bgra8",@"scale":@2,@"font":@24,@"allowEmpty":@YES,@"fullCrop":@YES},
        @{@"name":@"pixel-32x32",@"width":@32,@"height":@32,@"format":@"bgra8",@"scale":@2,@"font":@24,@"allowEmpty":@YES,@"fullCrop":@YES},
        @{@"name":@"pixel-64x64",@"width":@64,@"height":@64,@"format":@"bgra8",@"scale":@2,@"font":@24,@"allowEmpty":@YES,@"fullCrop":@YES},
        @{@"name":@"strip-4096x4",@"width":@4096,@"height":@4,@"format":@"bgra8",@"scale":@1,@"font":@24,@"allowEmpty":@YES,@"fullCrop":@YES},
        @{@"name":@"strip-4x4096",@"width":@4,@"height":@4096,@"format":@"bgra8",@"scale":@1,@"font":@24,@"allowEmpty":@YES,@"fullCrop":@YES},
        @{@"name":@"strip-4096x16",@"width":@4096,@"height":@16,@"format":@"bgra8",@"scale":@2,@"font":@24,@"allowEmpty":@YES,@"fullCrop":@YES},
        @{@"name":@"strip-16x4096",@"width":@16,@"height":@4096,@"format":@"bgra8",@"scale":@2,@"font":@24,@"allowEmpty":@YES,@"fullCrop":@YES}
    ];
    const char *filter=getenv("YURELAY_OCR_CASE");const char *repeats=getenv("YURELAY_OCR_REPEAT");int repeat=repeats?MAX(1,MIN(30,atoi(repeats))):1;BOOL all=YES;NSUInteger ran=0;
    const char *roundsValue=getenv("YURELAY_OCR_ROUNDS");int rounds=roundsValue?MAX(1,MIN(10,atoi(roundsValue))):1;
    for(int round=0;round<rounds;round++){printf("OCR matrix round=%d\n",round);
    for(NSDictionary *item in cases){if(filter&&![[[NSString stringWithUTF8String:filter] componentsSeparatedByString:@","] containsObject:item[@"name"]])continue;for(int run=0;run<repeat;run++){@autoreleasepool{
        size_t width=[item[@"width"] unsignedIntegerValue],height=[item[@"height"] unsignedIntegerValue];CGFloat scale=[item[@"scale"] doubleValue];BOOL delayed=[item[@"delayed"] boolValue];
        CGImageRef fixture=ADCreatePipelineFixture(width,height,item[@"format"],[item[@"font"] doubleValue],delayed);
        CGRect crop=[item[@"fullCrop"] boolValue]?CGRectMake(0,0,width/scale,height/scale):CGRectMake(13.25/scale,7.75/scale,(width-31.5)/scale,(height-21.25)/scale);
        printf("OCR pipeline begin case=%s repeat=%d\n",[item[@"name"] UTF8String],run);
        all=ADProbePipelineCase(item[@"name"],fixture,scale,crop,[item[@"allowEmpty"] boolValue])&&all;ran++;if(fixture)CGImageRelease(fixture);
    }}}
    }
    printf("OCR pipeline matrix cases=%lu passed=%s\n",(unsigned long)ran,all?"true":"false");return ran&&all?0:1;
}
