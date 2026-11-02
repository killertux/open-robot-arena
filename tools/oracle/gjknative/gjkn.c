/* gjkn: runs the ORIGINAL Robot Arena 2's distance query (FUN_0055c390) natively, 32-bit,
   on the host FPU, with the PE image mapped at its own virtual addresses.
   stdin: a header (x87 control word u32, mode u32: 0 the query, 1 the reduction) then records of BLOB
   bytes; stdout: each record after the call and the result as a double (the query's return value,
   or the reduction's al). argv[1]: the image made by pack_img.py (docs/29 section 12an). */
typedef unsigned int u32; typedef unsigned char u8;
static inline int sys(int n,int a,int b,int c,int d,int e,int f){int r;
 __asm__ volatile("push %%ebp; mov %7,%%ebp; int $0x80; pop %%ebp":"=a"(r):"a"(n),"b"(a),"c"(b),"d"(c),"S"(d),"D"(e),"m"(f):"memory");return r;}
#define BLOB 0x4000
#define BASE 0x30000000u
static volatile u32 gSA,gSB,gOUT,gST,gFN; static volatile double gRES;
static int rd_all(int fd,void*p,int n){int g=0;while(g<n){int r=sys(3,fd,(int)((u8*)p+g),n-g,0,0,0);if(r<=0)return g;g+=r;}return g;}
static int wr_all(int fd,const void*p,int n){int g=0;while(g<n){int r=sys(4,fd,(int)((const u8*)p+g),n-g,0,0,0);if(r<=0)return g;g+=r;}return g;}
void cstart(int *sp){
  int argc=sp[0]; char **argv=(char**)(sp+1);
  if(argc<2){sys(1,2,0,0,0,0,0);}
  int fd=sys(5,(int)argv[1],0,0,0,0,0);
  u32 hd[1]; rd_all(fd,hd,4); u32 n=hd[0];
  u32 ent[64]; rd_all(fd,ent,n*16);
  /* map sections */
  for(u32 i=0;i<n;i++){
    u32 va=ent[i*4],vs=ent[i*4+1],fo=ent[i*4+2],fl=ent[i*4+3];
    u32 sz=(vs+0xfff)&~0xfffu;
    sys(192,va,sz,7,0x32,-1,0); /* mmap2 FIXED|PRIVATE|ANON, RWX */
    /* read raw bytes at file offset fo */
    sys(19,fd,fo,0,0,0,0); /* lseek */
    rd_all(fd,(void*)va,fl);
  }
  /* patches */
  *(u8*)0x524d40=0xc3;
  static const u8 p4b0c50[]={0x8b,0x44,0x24,0x04, 0x8b,0x51,0x24,0x89,0x10, 0x8b,0x51,0x28,0x89,0x50,0x04, 0x8b,0x51,0x2c,0x89,0x50,0x08, 0xc2,0x04,0x00};
  for(u32 i=0;i<sizeof p4b0c50;i++)((u8*)0x4b0c50)[i]=p4b0c50[i];
  /* scratch blob */
  sys(192,BASE,BLOB+0x1000,7,0x32,-1,0);
  u32 cw, mode; rd_all(0,&cw,4); rd_all(0,&mode,4);
  unsigned short cws=(unsigned short)cw;
  __asm__ volatile("fldcw %0"::"m"(cws));
  u32 SA=BASE+0x3000, SB=BASE+0x3100, OUT=BASE+0x3200, ST=BASE+0x3300;
  for(;;){
    if(rd_all(0,(void*)BASE,BLOB)!=BLOB) break;
    if(mode==0){
    gSA=SA;gSB=SB;gOUT=OUT;gST=ST;gFN=0x55c390;
    __asm__ volatile(
      "pushl gST; pushl gOUT; pushl gSB; pushl gSA; call *gFN; addl $16,%%esp; fstpl gRES"
      :::"eax","ecx","edx","memory","cc","st","st(1)","st(2)","st(3)","st(4)","st(5)","st(6)","st(7)");
    } else {
    gSA=BASE+0x100;gSB=BASE+0x200;gOUT=BASE+0x300;gFN=0x55f100;
    __asm__ volatile(
      "pushl gOUT; pushl gSB; pushl gSA; call *gFN; addl $12,%%esp; movzbl %%al,%%eax; movl %%eax,gRES; fildl gRES; fstpl gRES"
      :::"eax","ecx","edx","memory","cc","st","st(1)","st(2)","st(3)","st(4)","st(5)","st(6)","st(7)");
    }
    wr_all(1,(void*)BASE,BLOB);
    wr_all(1,(void*)&gRES,8);
  }
  sys(1,0,0,0,0,0,0);
}
__asm__(".globl _start\n_start:\n mov %esp,%eax\n push %eax\n call cstart\n");
