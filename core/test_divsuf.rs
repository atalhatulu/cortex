fn main() {
    let data = b"baab";
    let (pidx, bwt) = divsufsort::bwt(data);
    println!("pidx: {}, bwt: {:?}", pidx, bwt);
}
