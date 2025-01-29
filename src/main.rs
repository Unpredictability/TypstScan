use tex2typst_rs::tex2typst;

fn main() {
    let tex1 = "i_D = \\mu_n C_\\text{ox} \\frac{W}{L} \\left[ (v_\\text{GS} - V_t)v_\\text{DS} - \\frac{1}{2} v_\\text{DS}^2 \\right]";
    let tex2 = "\\iint_{\\Sigma} \\operatorname{curl}(\\vec{F}) \\cdot \\mathrm{d}\\vec{S} = \\oint_{\\partial \\Sigma} \\vec{F} \\times \\mathrm{d}\\vec{l}";
    println!("{}", tex2typst(tex1));
    println!("{}", tex2typst(tex2));
}
