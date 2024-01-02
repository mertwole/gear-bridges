//
// Copyright 2017 Christian Reitwiessner
// Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:
// The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//
// 2019 OKIMS
//      ported to solidity 0.6
//      fixed linter warnings
//      added requiere error messages
//
//
// SPDX-License-Identifier: GPL-3.0
pragma solidity ^0.6.11;
library Pairing {
    struct G1Point {
        uint X;
        uint Y;
    }
    // Encoding of field elements is: X[0] * z + X[1]
    struct G2Point {
        uint[2] X;
        uint[2] Y;
    }
    /// @return the generator of G1
    function P1() internal pure returns (G1Point memory) {
        return G1Point(1, 2);
    }
    /// @return the generator of G2
    function P2() internal pure returns (G2Point memory) {
        // Original code point
        return G2Point(
            [11559732032986387107991004021392285783925812861821192530917403151452391805634,
             10857046999023057135944570762232829481370756359578518086990519993285655852781],
            [4082367875863433681332203403145435568316851327593401208105741076214120093531,
             8495653923123431417604973247489272438418190587263600148770280649306958101930]
        );

/*
        // Changed by Jordi point
        return G2Point(
            [10857046999023057135944570762232829481370756359578518086990519993285655852781,
             11559732032986387107991004021392285783925812861821192530917403151452391805634],
            [8495653923123431417604973247489272438418190587263600148770280649306958101930,
             4082367875863433681332203403145435568316851327593401208105741076214120093531]
        );
*/
    }
    /// @return r the negation of p, i.e. p.addition(p.negate()) should be zero.
    function negate(G1Point memory p) internal pure returns (G1Point memory r) {
        // The prime q in the base field F_q for G1
        uint q = 21888242871839275222246405745257275088696311157297823662689037894645226208583;
        if (p.X == 0 && p.Y == 0)
            return G1Point(0, 0);
        return G1Point(p.X, q - (p.Y % q));
    }
    /// @return r the sum of two points of G1
    function addition(G1Point memory p1, G1Point memory p2) internal view returns (G1Point memory r) {
        uint[4] memory input;
        input[0] = p1.X;
        input[1] = p1.Y;
        input[2] = p2.X;
        input[3] = p2.Y;
        bool success;
        // solium-disable-next-line security/no-inline-assembly
        assembly {
            success := staticcall(sub(gas(), 2000), 6, input, 0xc0, r, 0x60)
            // Use "invalid" to make gas estimation work
            switch success case 0 { invalid() }
        }
        require(success,"pairing-add-failed");
    }
    /// @return r the product of a point on G1 and a scalar, i.e.
    /// p == p.scalar_mul(1) and p.addition(p) == p.scalar_mul(2) for all points p.
    function scalar_mul(G1Point memory p, uint s) internal view returns (G1Point memory r) {
        uint[3] memory input;
        input[0] = p.X;
        input[1] = p.Y;
        input[2] = s;
        bool success;
        // solium-disable-next-line security/no-inline-assembly
        assembly {
            success := staticcall(sub(gas(), 2000), 7, input, 0x80, r, 0x60)
            // Use "invalid" to make gas estimation work
            switch success case 0 { invalid() }
        }
        require (success,"pairing-mul-failed");
    }
    /// @return the result of computing the pairing check
    /// e(p1[0], p2[0]) *  .... * e(p1[n], p2[n]) == 1
    /// For example pairing([P1(), P1().negate()], [P2(), P2()]) should
    /// return true.
    function pairing(G1Point[] memory p1, G2Point[] memory p2) internal view returns (bool) {
        require(p1.length == p2.length,"pairing-lengths-failed");
        uint elements = p1.length;
        uint inputSize = elements * 6;
        uint[] memory input = new uint[](inputSize);
        for (uint i = 0; i < elements; i++)
        {
            input[i * 6 + 0] = p1[i].X;
            input[i * 6 + 1] = p1[i].Y;
            input[i * 6 + 2] = p2[i].X[0];
            input[i * 6 + 3] = p2[i].X[1];
            input[i * 6 + 4] = p2[i].Y[0];
            input[i * 6 + 5] = p2[i].Y[1];
        }
        uint[1] memory out;
        bool success;
        // solium-disable-next-line security/no-inline-assembly
        assembly {
            success := staticcall(sub(gas(), 2000), 8, add(input, 0x20), mul(inputSize, 0x20), out, 0x20)
            // Use "invalid" to make gas estimation work
            switch success case 0 { invalid() }
        }
        require(success,"pairing-opcode-failed");
        return out[0] != 0;
    }
    /// Convenience method for a pairing check for two pairs.
    function pairingProd2(G1Point memory a1, G2Point memory a2, G1Point memory b1, G2Point memory b2) internal view returns (bool) {
        G1Point[] memory p1 = new G1Point[](2);
        G2Point[] memory p2 = new G2Point[](2);
        p1[0] = a1;
        p1[1] = b1;
        p2[0] = a2;
        p2[1] = b2;
        return pairing(p1, p2);
    }
    /// Convenience method for a pairing check for three pairs.
    function pairingProd3(
            G1Point memory a1, G2Point memory a2,
            G1Point memory b1, G2Point memory b2,
            G1Point memory c1, G2Point memory c2
    ) internal view returns (bool) {
        G1Point[] memory p1 = new G1Point[](3);
        G2Point[] memory p2 = new G2Point[](3);
        p1[0] = a1;
        p1[1] = b1;
        p1[2] = c1;
        p2[0] = a2;
        p2[1] = b2;
        p2[2] = c2;
        return pairing(p1, p2);
    }
    /// Convenience method for a pairing check for four pairs.
    function pairingProd4(
            G1Point memory a1, G2Point memory a2,
            G1Point memory b1, G2Point memory b2,
            G1Point memory c1, G2Point memory c2,
            G1Point memory d1, G2Point memory d2
    ) internal view returns (bool) {
        G1Point[] memory p1 = new G1Point[](4);
        G2Point[] memory p2 = new G2Point[](4);
        p1[0] = a1;
        p1[1] = b1;
        p1[2] = c1;
        p1[3] = d1;
        p2[0] = a2;
        p2[1] = b2;
        p2[2] = c2;
        p2[3] = d2;
        return pairing(p1, p2);
    }
}
contract Verifier {
    using Pairing for *;
    struct VerifyingKey {
        Pairing.G1Point alfa1;
        Pairing.G2Point beta2;
        Pairing.G2Point gamma2;
        Pairing.G2Point delta2;
        Pairing.G1Point[] IC;
    }
    struct Proof {
        Pairing.G1Point A;
        Pairing.G2Point B;
        Pairing.G1Point C;
    }
    function verifyingKey() internal pure returns (VerifyingKey memory vk) {
        vk.alfa1 = Pairing.G1Point(
            20491192805390485299153009773594534940189261866228447918068658471970481763042,
            9383485363053290200918347156157836566562967994039712273449902621266178545958
        );

        vk.beta2 = Pairing.G2Point(
            [4252822878758300859123897981450591353533073413197771768651442665752259397132,
             6375614351688725206403948262868962793625744043794305715222011528459656738731],
            [21847035105528745403288232691147584728191162732299865338377159692350059136679,
             10505242626370262277552901082094356697409835680220590971873171140371331206856]
        );
        vk.gamma2 = Pairing.G2Point(
            [11559732032986387107991004021392285783925812861821192530917403151452391805634,
             10857046999023057135944570762232829481370756359578518086990519993285655852781],
            [4082367875863433681332203403145435568316851327593401208105741076214120093531,
             8495653923123431417604973247489272438418190587263600148770280649306958101930]
        );
        vk.delta2 = Pairing.G2Point(
            [8941023087375379362716473273592461580659450141042041154499474200294982081832,
             3104109724546371990145788487045488126344136676825080790725230946441302200563],
            [9545981983063026251529999957187900721808783489373608297137693853551123457883,
             6923257928748075981761171964303722437583346529146929257538721179054554326616]
        );
        vk.IC = new Pairing.G1Point[](69);
        
        vk.IC[0] = Pairing.G1Point( 
            18287773998892933121473782861374141318834021015733195472694072571201768089146,
            17339028216566286287854937804429234773598087263765793310968760771267770779685
        );                                      
        
        vk.IC[1] = Pairing.G1Point( 
            18376792320321188194899627306945814878613935402363966002383424102645085468308,
            5246860833724749084985819089725107529090698233908657976237931862771601843607
        );                                      
        
        vk.IC[2] = Pairing.G1Point( 
            10432156559670550130427514595739387323202671293961497946509075737936665781299,
            8655433479016964503859198275798699913837104647196452717109937092012308604041
        );                                      
        
        vk.IC[3] = Pairing.G1Point( 
            3623936869933443429452837442315478628088628722571125750751357433178961183767,
            19060204376999373256279436718820216353301534513852546477026013911124387984522
        );                                      
        
        vk.IC[4] = Pairing.G1Point( 
            16171070722135377164830498405225177319939728791186430105269009182824777042035,
            1845809376090769920938782047673230350873039663346908165889307791532376951082
        );                                      
        
        vk.IC[5] = Pairing.G1Point( 
            10488331845114771816365553078410579224798452510281999389111081424006815745079,
            20628016133933744867308204874162162616138000697335720721704299532189068240687
        );                                      
        
        vk.IC[6] = Pairing.G1Point( 
            21546444857140931060287761289751895683288342865670318003496342524994580788979,
            4290213380162098017353462162832267676644546347238093871257639703667392440381
        );                                      
        
        vk.IC[7] = Pairing.G1Point( 
            6006147236737291621596403987067840471569941233592552359887238584502592594931,
            14719852607064127614939704420429519106824387661469058595406308851773643996474
        );                                      
        
        vk.IC[8] = Pairing.G1Point( 
            16679236304151401124880185419812776939427111677230471149427273798362489891141,
            14037694606123486823963855121655314418824755313725845310318566417026994965444
        );                                      
        
        vk.IC[9] = Pairing.G1Point( 
            11086390559400265707739937030002650108885181552021949536346193603208909520900,
            10034050030385972487266029814114297383553393053458577883712522890977217272415
        );                                      
        
        vk.IC[10] = Pairing.G1Point( 
            10183355428170068129468069509293104514443992967318833979402416763114667279537,
            5045474536705284504931119732801290112679532082087409440801495618737001765081
        );                                      
        
        vk.IC[11] = Pairing.G1Point( 
            15038994942071293131863786165974565366665327133884717632749414848845987409057,
            18689759094857504806121037369343609325572743585418964895593905736712991376372
        );                                      
        
        vk.IC[12] = Pairing.G1Point( 
            19472280478609480340824715034114443100703281677512051746661785256644498715533,
            11493845311005476065823572764458641916673647644382047826478195804090220962051
        );                                      
        
        vk.IC[13] = Pairing.G1Point( 
            16833039205321837434529408954530693005963459333126898859201183952542427466127,
            7901209485612659591779846915565319528700635263330385896890975135486677907754
        );                                      
        
        vk.IC[14] = Pairing.G1Point( 
            14393002327446041151002788147780672866959732233778036879183253008585168685708,
            7112709399513281801715469455300112599723433847582198493320072541088439072008
        );                                      
        
        vk.IC[15] = Pairing.G1Point( 
            19805857017049921350200739605148119472570718415611059117888291883833634493282,
            8599680762948296217314679165014667635993216939506287496345637053734729177463
        );                                      
        
        vk.IC[16] = Pairing.G1Point( 
            18868706057646243005331347069010979344298450768534814744550406875787794977737,
            10222111646273970298728301839837513482686656530315556882188145913647297974806
        );                                      
        
        vk.IC[17] = Pairing.G1Point( 
            8136914497788670911082649462142136431001692940956691690266374237507560598422,
            3926914613423619322838192198896184996686743534021757135960607369479902027425
        );                                      
        
        vk.IC[18] = Pairing.G1Point( 
            8182625056279388427318778488454081044973274110026547239634491959037155810102,
            12177713045637654701996433473704906280077161490234846209345841103566930422262
        );                                      
        
        vk.IC[19] = Pairing.G1Point( 
            50293319681530899735195957470099198747422537494379100917629655493622477652,
            4264013340851297634165586087241719343838169836048229326271169148307856113969
        );                                      
        
        vk.IC[20] = Pairing.G1Point( 
            6006568546235700285312984341750634227354581535790022617139557437515948790453,
            18740932532342210775199900287982029671551649232771913499275732594389434007074
        );                                      
        
        vk.IC[21] = Pairing.G1Point( 
            10012019210649847918896736548117429108722607833418503138877837584489788722561,
            9365671522720172915669804792408427933086030125271273184775515934599057522397
        );                                      
        
        vk.IC[22] = Pairing.G1Point( 
            13352011922615373407390051981123755480110583865275213061361517807604479440724,
            8620937979570900212948385082860177036296589543031077007683655105989828072775
        );                                      
        
        vk.IC[23] = Pairing.G1Point( 
            15844630399078297218328389525203886417301159388055438971311738382909998440924,
            8577458887204014327195957864558355789105893484346568075210122042714841870125
        );                                      
        
        vk.IC[24] = Pairing.G1Point( 
            18494947619027931586157751213556938213233156827147844807348184911375880861324,
            13888420953376555910983674053523511786859115388790623162357674936021267811924
        );                                      
        
        vk.IC[25] = Pairing.G1Point( 
            7190107987282917606346408413361777351811840695666150684424528660485129451852,
            13778356077003196697839585109386758656869199806467467537546024372448447496846
        );                                      
        
        vk.IC[26] = Pairing.G1Point( 
            15126740317662144764415746629808017320520330089182000721846743932409154036431,
            14846691722311321559944620163171175772714901490199014838816675471623388044603
        );                                      
        
        vk.IC[27] = Pairing.G1Point( 
            13500239365587454118313136350048011494992501923983223248719802632896076168211,
            17810221637197754521314160653703463011026382891427952572630258012032844884740
        );                                      
        
        vk.IC[28] = Pairing.G1Point( 
            19825741942700794955038641078611510032128783332495696675195575702714422113413,
            6151571109755058226384256486458253695600518753469885987829827212322779053512
        );                                      
        
        vk.IC[29] = Pairing.G1Point( 
            18826065203343126107646900765300876980290316135098412306232831330303023166948,
            7010639986429274567357767193134185486351819782436099359940285114424542565117
        );                                      
        
        vk.IC[30] = Pairing.G1Point( 
            6020982855735718016068420159345374475735448698901038433845916179450321929201,
            12797225506907159047297000635731679535406700200442504207329822892278999939781
        );                                      
        
        vk.IC[31] = Pairing.G1Point( 
            4614825237268118168662235105861595583700407811590047756422414429953111172221,
            12000512262411460257251079739354477897257500277350243438665293573881646750523
        );                                      
        
        vk.IC[32] = Pairing.G1Point( 
            8872207669452060378104853381884413624746442408120898594784277567707808566726,
            13548971064222844784778378710651880825236113680685476774406754199944634970801
        );                                      
        
        vk.IC[33] = Pairing.G1Point( 
            14419756999203607354379322891211145985994238415268333810663315544527175086981,
            13135012404507674328683021925296985316008203747451041603777306103893498304402
        );                                      
        
        vk.IC[34] = Pairing.G1Point( 
            19180645682429471681654935844084064112216300900343057411167337931611174519487,
            7268263511398434417549042029093668752736108852862675108399726831954046503739
        );                                      
        
        vk.IC[35] = Pairing.G1Point( 
            2699674508173992670392847845711212183608064364697087218964997077616054611263,
            19643627760044995213968224667133301938452872616773009238459621047362691292436
        );                                      
        
        vk.IC[36] = Pairing.G1Point( 
            13844518714767315052062182752467591911704831038992717200450849234859735978844,
            9269070836734409008943628239763405911554317628406983441614417818049237804186
        );                                      
        
        vk.IC[37] = Pairing.G1Point( 
            5329498077541088544494616479473003009112831235240569852990637867342468136405,
            8685429708635612632343089377246951528927322688003872976761310861690869499800
        );                                      
        
        vk.IC[38] = Pairing.G1Point( 
            21391952633870014278489415220345529606476700651367698737162589855871957124767,
            3501816141804799831245954704619388619873990421149028378403511040228834277398
        );                                      
        
        vk.IC[39] = Pairing.G1Point( 
            7475185089350201884083327680094658702911738963239809311383102055335639971922,
            18845196660000762180528599932121921214882554571203498399017155015205056321184
        );                                      
        
        vk.IC[40] = Pairing.G1Point( 
            3432741474489800819928336110255802556999271724716254069540358457372665332683,
            21612402902087487345277844451599724315377705921360254542278967760591493316689
        );                                      
        
        vk.IC[41] = Pairing.G1Point( 
            20990986202056529205655538515329136905962664658420842900782198076425669697819,
            4978979424517994806895538585379355115152087079656035502598169854922128090157
        );                                      
        
        vk.IC[42] = Pairing.G1Point( 
            7232162352347871551892764907230233659863883315807837540101614459974666958535,
            15242938213204610459268249552260592814317844119198101559672955512263645254009
        );                                      
        
        vk.IC[43] = Pairing.G1Point( 
            3505840598904502929628448900760201133109010855063965888058392543255587418760,
            9084154115047018672487014436988840456907998848073624704100808326717811841056
        );                                      
        
        vk.IC[44] = Pairing.G1Point( 
            16906249390766603718416233034451820773406107405984613013543808090051001791038,
            8565772484608233084973192993690341670035075748353666104181475969872363942198
        );                                      
        
        vk.IC[45] = Pairing.G1Point( 
            13117567624718937339434339494626428785255661497011892326704491510960398267072,
            13757450224792318531314629974442362755075598855522947665021729554542875973974
        );                                      
        
        vk.IC[46] = Pairing.G1Point( 
            9113656145886311689908300711354868527291086833333476269581939980337646376015,
            21464357854953664062198979325605574019631159535900769123565111462676930701157
        );                                      
        
        vk.IC[47] = Pairing.G1Point( 
            15463261234800146108563469216007344787420331756933969667557348852044216669616,
            7248906820261207277160765590424745743934511898524346787162426370092926271574
        );                                      
        
        vk.IC[48] = Pairing.G1Point( 
            18577454558343278745772248667442956884256510942329474743845411577925919512583,
            10087034310275324886561534796517137739506173576740024073851882538161290955202
        );                                      
        
        vk.IC[49] = Pairing.G1Point( 
            19614764909050170772081125716059404768201902587710472688182068391095563240022,
            3708027028932860620304142283937273965185054763396464791545405511516272211532
        );                                      
        
        vk.IC[50] = Pairing.G1Point( 
            7148164402461812425116785669013018465133348632813322573197766013448208221711,
            15136166878533161238625224495328920799193650925066485589157612405966096372282
        );                                      
        
        vk.IC[51] = Pairing.G1Point( 
            14256402083298336444104787337017961893191247485551417949416228264548458472571,
            5882956469438007925029145094313460564192916863022627393024271378855070817371
        );                                      
        
        vk.IC[52] = Pairing.G1Point( 
            12086039176602257534751737252882028150929861923605810973590122556654703848794,
            3413636873050210254996456675139935742618659898112258072607293263427395540405
        );                                      
        
        vk.IC[53] = Pairing.G1Point( 
            21777807273667091845249463069585246452385350259922640706915530364561679551157,
            10737676285769852493813268991488861335751373685457017510566080473489200639348
        );                                      
        
        vk.IC[54] = Pairing.G1Point( 
            6303387909370154488445447725688746088404425312261334121630858495833982212172,
            726047506282826007835049912744848612519923063428614115668024376343293103790
        );                                      
        
        vk.IC[55] = Pairing.G1Point( 
            6599946311956303739665050436074379566714549447801981979175315503486104198419,
            19107753254870402352178800405458254469682796740583796457886945893564697227136
        );                                      
        
        vk.IC[56] = Pairing.G1Point( 
            7032546690905213939532864800773579423132137343270771690719085178931221107197,
            12750313382602642329905642177862011418440317187692135471133659482727739297508
        );                                      
        
        vk.IC[57] = Pairing.G1Point( 
            6156557944248115439462310909283968277197285762217294236617519251419836008631,
            10558980377200062505765086679757495563827268177876576689479852944855404655274
        );                                      
        
        vk.IC[58] = Pairing.G1Point( 
            3212626902497833264670637200918253778816179971744021001206861405487986432114,
            7840362310530433602128475684730462107098007248997799661984241657792302061098
        );                                      
        
        vk.IC[59] = Pairing.G1Point( 
            8471219052046365649642274411474929956566770137302942062313234832320994037200,
            13690476727132266322181892034252475710940060992195703052938212006549168397780
        );                                      
        
        vk.IC[60] = Pairing.G1Point( 
            14560747329251667405077804154254733595886466390772536164739080314790476889235,
            18779682220504687296532328196982127644765505105367922941301314326095392287487
        );                                      
        
        vk.IC[61] = Pairing.G1Point( 
            4876673219112607424541461148209296982579445622793711167262143933132682039647,
            2094620946869654684167881495378686887087445131613006340334780679275271509771
        );                                      
        
        vk.IC[62] = Pairing.G1Point( 
            5789272364267457590184881311580262279516781023482324409334394238832711865609,
            1523488993000768918589030095192032563909672708340768478222218736816603930834
        );                                      
        
        vk.IC[63] = Pairing.G1Point( 
            1708109118396373753518860338859406098934903002371287061825739318169916184780,
            4438616150167811412609048704398059950467888141851477478267747211067394536791
        );                                      
        
        vk.IC[64] = Pairing.G1Point( 
            14190394571776937089365913893812474766710838874935554086946923059828691973292,
            237277588418572883722478302842457712279647428412509533048121005230126249948
        );                                      
        
        vk.IC[65] = Pairing.G1Point( 
            20815906924717862407607201007203377002023003457822447841863905082989517401068,
            13925814352358849974938615414115819258422443995706049017053319712183533610190
        );                                      
        
        vk.IC[66] = Pairing.G1Point( 
            3632503952573402145759164102702971498804821322880554581855447931297784840316,
            12292104810515245801317986300451699358281035390552333362991869475199246471873
        );                                      
        
        vk.IC[67] = Pairing.G1Point( 
            12624276678968762559585910545433288202716373604931489320599619177101063125529,
            12888930332129052920916114489708032257716169482308488933151522391850578421099
        );                                      
        
        vk.IC[68] = Pairing.G1Point( 
            7746092648981233063717520395041653221298692400309401513396142621627675636520,
            1709031485763953009429441379317725977056566127810581983147724754498161718347
        );                                      
        
    }
    function verify(uint[] memory input, Proof memory proof) internal view returns (uint) {
        uint256 snark_scalar_field = 21888242871839275222246405745257275088548364400416034343698204186575808495617;
        VerifyingKey memory vk = verifyingKey();
        require(input.length + 1 == vk.IC.length,"verifier-bad-input");
        // Compute the linear combination vk_x
        Pairing.G1Point memory vk_x = Pairing.G1Point(0, 0);
        for (uint i = 0; i < input.length; i++) {
            require(input[i] < snark_scalar_field,"verifier-gte-snark-scalar-field");
            vk_x = Pairing.addition(vk_x, Pairing.scalar_mul(vk.IC[i + 1], input[i]));
        }
        vk_x = Pairing.addition(vk_x, vk.IC[0]);
        if (!Pairing.pairingProd4(
            Pairing.negate(proof.A), proof.B,
            vk.alfa1, vk.beta2,
            vk_x, vk.gamma2,
            proof.C, vk.delta2
        )) return 1;
        return 0;
    }
    /// @return r  bool true if proof is valid
    function verifyProof(
            uint[2] memory a,
            uint[2][2] memory b,
            uint[2] memory c,
            uint[68] memory input
        ) public view returns (bool r) {
        Proof memory proof;
        proof.A = Pairing.G1Point(a[0], a[1]);
        proof.B = Pairing.G2Point([b[0][0], b[0][1]], [b[1][0], b[1][1]]);
        proof.C = Pairing.G1Point(c[0], c[1]);
        uint[] memory inputValues = new uint[](input.length);
        for(uint i = 0; i < input.length; i++){
            inputValues[i] = input[i];
        }
        if (verify(inputValues, proof) == 0) {
            return true;
        } else {
            return false;
        }
    }
}
